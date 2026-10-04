//! The table-driven state machine (`fsm_init` 0x8006ba60, `fsm_post`
//! 0x8006baac, `fsm_tick` 0x8006bb18).
//!
//! States are data: lists of actions to run on entering, on each tick, and on
//! leaving, and event transitions. `A` is whatever names an action; the
//! tables read from the executable use the original function addresses.
//! The timing is the original's, one phase per tick, including the shortcut
//! a state with no exit actions takes inside its update.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use hwtr_psx::Memory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State<A> {
    pub enter: Vec<A>,
    pub update: Vec<A>,
    pub exit: Vec<A>,
    /// (event, target state)
    pub transitions: Vec<(i16, i16)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Def<A> {
    pub states: Vec<State<A>>,
    pub initial: i16,
    pub last: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Enter = 0,
    Update = 1,
    Exit = 2,
    Done = 3,
}

/// A running machine. Mirrors the 24-byte record: `current`, `next`, `phase`
/// and the pending `event`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fsm {
    pub current: usize,
    pub next: Option<usize>,
    pub last: usize,
    pub phase: Phase,
    /// -1 for none.
    pub event: i16,
}

/// What an action gets: a way to post an event to the machine running it.
pub struct Poster<'a> {
    transitions: &'a [(i16, i16)],
    event: &'a mut i16,
}

impl Poster<'_> {
    pub fn post(&mut self, event: i16) {
        post(self.transitions, self.event, event);
    }

    /// Whether the running state has a transition on `event` (0x8009e270).
    pub fn takes(&self, event: i16) -> bool {
        self.transitions.iter().any(|t| t.0 == event)
    }
}

/// Accepts `event` only if none is pending and the state has a transition on
/// it; otherwise drops it.
fn post(transitions: &[(i16, i16)], pending: &mut i16, event: i16) {
    if *pending == -1 && transitions.iter().any(|t| t.0 == event) {
        *pending = event;
    }
}

impl Fsm {
    pub fn new<A>(def: &Def<A>) -> Fsm {
        Fsm { current: def.initial as usize, next: None, last: def.last as usize, phase: Phase::Enter, event: -1 }
    }

    pub fn post<A>(&mut self, def: &Def<A>, event: i16) {
        post(&def.states[self.current].transitions, &mut self.event, event);
    }

    /// Runs one phase. `run` performs an action. Returns true once the final
    /// state has exited (the original returns 2 then, 0 otherwise).
    pub fn tick<A>(&mut self, def: &Def<A>, mut run: impl FnMut(&A, &mut Poster)) -> bool {
        let state = |i: usize| &def.states[i];
        let mut call_all = |fsm: &mut Fsm, list: &[A], cur: usize| {
            for a in list {
                let mut p = Poster { transitions: &state(cur).transitions, event: &mut fsm.event };
                run(a, &mut p);
            }
        };
        match self.phase {
            Phase::Done => return true,
            Phase::Enter => {
                let cur = self.current;
                call_all(self, &state(cur).enter, cur);
                self.phase = Phase::Update;
            }
            Phase::Update => {
                let cur = self.current;
                call_all(self, &state(cur).update, cur);
                if self.current == self.last {
                    self.phase = Phase::Exit;
                } else if self.event != -1 {
                    // The original re-reads `current` on every iteration, so
                    // after an immediate switch it scans the new state's list;
                    // `event` is -1 by then and nothing more matches.
                    let mut k = 0;
                    while k < state(self.current).transitions.len() {
                        let (ev, target) = state(self.current).transitions[k];
                        if ev == self.event {
                            let next = target as usize;
                            self.phase = Phase::Exit;
                            self.event = -1;
                            self.next = Some(next);
                            if state(self.current).exit.is_empty() {
                                if self.current == self.last {
                                    self.phase = Phase::Done;
                                } else {
                                    self.current = next;
                                    self.phase =
                                        if state(next).enter.is_empty() { Phase::Update } else { Phase::Enter };
                                }
                            }
                        }
                        k += 1;
                    }
                }
            }
            Phase::Exit => {
                let cur = self.current;
                call_all(self, &state(cur).exit, cur);
                if self.current == self.last {
                    self.phase = Phase::Done;
                } else {
                    let next = self.next.expect("exit without a transition");
                    self.current = next;
                    self.phase = if state(next).enter.is_empty() { Phase::Update } else { Phase::Enter };
                }
            }
        }
        self.phase == Phase::Done
    }
}

/// Reads a machine definition out of the executable: the machine record at
/// `addr` and the state records it points at, the action lists as function
/// addresses. The state count is not stored; the state table runs up to the
/// machine record, which is how both machines in `CCCPSX.EXE` are laid out.
pub fn read_def(mem: &Memory, addr: u32) -> Option<Def<u32>> {
    read_def_bytes(&|a| mem.u8(a).unwrap_or(0), addr)
}

/// [`read_def`] from `byte`, which reads the executable at an address.
pub fn read_def_bytes(byte: &dyn Fn(u32) -> u8, addr: u32) -> Option<Def<u32>> {
    let u16_at = |a: u32| u16::from_le_bytes([byte(a), byte(a + 1)]);
    let u32_at = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
    let table = u32_at(addr);
    if table >= addr || !(addr - table).is_multiple_of(4) {
        return None;
    }
    let count = (addr - table) / 4;
    let list = |at: u32, n: i8| -> Vec<u32> { (0..n.max(0) as u32).map(|i| u32_at(at + 4 * i)).collect() };
    let mut states = Vec::with_capacity(count as usize);
    for i in 0..count {
        let s = u32_at(table + 4 * i);
        let n = |k: u32| byte(s + 16 + k) as i8;
        let (ne, nu, nx, nt) = (n(0), n(1), n(2), n(3));
        let trans_at = u32_at(s + 12);
        let transitions = (0..nt.max(0) as u32)
            .map(|k| (u16_at(trans_at + 4 * k) as i16, u16_at(trans_at + 4 * k + 2) as i16))
            .collect();
        states.push(State {
            enter: list(u32_at(s), ne),
            update: list(u32_at(s + 4), nu),
            exit: list(u32_at(s + 8), nx),
            transitions,
        });
    }
    Some(Def { states, initial: u16_at(addr + 4) as i16, last: u16_at(addr + 6) as i16 })
}

/// A handler for one of the original's actions.
pub type Action<K> = fn(&mut K, &mut Poster);

/// A kit's actions by the original's function addresses, each registered by
/// the part of the program it belongs to.
pub struct Registry<K> {
    actions: BTreeMap<u32, Action<K>>,
}

impl<K> Default for Registry<K> {
    fn default() -> Self {
        Registry { actions: BTreeMap::new() }
    }
}

impl<K> Registry<K> {
    /// Registers `action` for `addr`. An address registered twice is a
    /// mistake in the port.
    pub fn add(&mut self, addr: u32, action: Action<K>) -> &mut Self {
        let old = self.actions.insert(addr, action);
        assert!(old.is_none(), "action {addr:#010x} registered twice");
        self
    }

    pub fn get(&self, addr: u32) -> Option<Action<K>> {
        self.actions.get(&addr).copied()
    }

    /// The actions `def` names that nothing has registered: what of the
    /// machine is not ported.
    pub fn missing(&self, def: &Def<u32>) -> BTreeSet<u32> {
        def.states
            .iter()
            .flat_map(|s| s.enter.iter().chain(&s.update).chain(&s.exit))
            .filter(|a| !self.actions.contains_key(a))
            .copied()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// A program one of the game's state machines runs: the front end
/// (`fsm_main`) or the pause menu (`fsm_second`). Each has its own actions,
/// named by the original's function addresses; the stepping is shared. The
/// original's main loop (0x80010a5c) steps as fast as it can and waits for
/// the vertical blank only when a frame is finished, so a blank is every
/// step until then.
pub trait Kit: Sized {
    /// The machine's definition, and where it is.
    fn def(&self) -> Rc<Def<u32>>;
    fn machine(&mut self) -> &mut Fsm;
    /// The ported actions.
    fn actions(&self) -> Rc<Registry<Self>>;
    /// An action the machine named that is not ported.
    fn unported(&mut self, addr: u32);

    /// Runs the action at `addr`.
    fn act(&mut self, addr: u32, p: &mut Poster) {
        match self.actions().get(addr) {
            Some(action) => action(self, p),
            None => self.unported(addr),
        }
    }
    /// Whether this blank is over: a frame was finished, or the program
    /// has something to hand over.
    fn blank_over(&self) -> bool;

    /// One blank: steps until it is over, at most `cap` of them (for states
    /// that never draw). True once the machine has finished.
    fn run_blank(&mut self, cap: usize) -> bool {
        let def = self.def();
        for _ in 0..cap {
            let mut fsm = self.machine().clone();
            let done = fsm.tick(&def, |&a, p| self.act(a, p));
            *self.machine() = fsm;
            if done {
                return true;
            }
            if self.blank_over() {
                break;
            }
        }
        false
    }
}
