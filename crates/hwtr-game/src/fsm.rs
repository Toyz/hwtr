//! The table-driven state machine (`fsm_init` 0x8006ba60, `fsm_post`
//! 0x8006baac, `fsm_tick` 0x8006bb18).
//!
//! States are data: lists of actions to run on entering, on each tick, and on
//! leaving, and event transitions. `A` is whatever names an action; the
//! tables read from the executable use the original function addresses.
//! The timing is the original's, one phase per tick, including the shortcut
//! a state with no exit actions takes inside its update.

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
    let table = mem.u32(addr)?;
    if table >= addr || !(addr - table).is_multiple_of(4) {
        return None;
    }
    let count = (addr - table) / 4;
    let list = |at: u32, n: i8| -> Option<Vec<u32>> { (0..n.max(0) as u32).map(|i| mem.u32(at + 4 * i)).collect() };
    let mut states = Vec::with_capacity(count as usize);
    for i in 0..count {
        let s = mem.u32(table + 4 * i)?;
        let n = |k: u32| mem.u8(s + 16 + k).map(|b| b as i8);
        let (ne, nu, nx, nt) = (n(0)?, n(1)?, n(2)?, n(3)?);
        let trans_at = mem.u32(s + 12)?;
        let transitions = (0..nt.max(0) as u32)
            .map(|k| Some((mem.u16(trans_at + 4 * k)? as i16, mem.u16(trans_at + 4 * k + 2)? as i16)))
            .collect::<Option<Vec<_>>>()?;
        states.push(State {
            enter: list(mem.u32(s)?, ne)?,
            update: list(mem.u32(s + 4)?, nu)?,
            exit: list(mem.u32(s + 8)?, nx)?,
            transitions,
        });
    }
    Some(Def { states, initial: mem.u16(addr + 4)? as i16, last: mem.u16(addr + 6)? as i16 })
}
