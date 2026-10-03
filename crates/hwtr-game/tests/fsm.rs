//! The state machine against fsm_init, fsm_post and fsm_tick.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use hwtr_cpu::Machine;
use hwtr_game::fsm::{self, Fsm, Phase};

const FSM_INIT: u32 = 0x8006_ba60;
const FSM_POST: u32 = 0x8006_baac;
const FSM_TICK: u32 = 0x8006_bb18;
const MACHINES: [u32; 2] = [0x800c_5bdc, 0x800c_6640];

/// The original machine record's fields, as the port names them.
fn original_state(m: &mut Machine, addr: u32, table: u32, def_len: usize) -> (usize, Phase, i16) {
    let cur = m.bus.read_u32(addr + 8);
    let index = (0..def_len as u32).find(|&i| m.bus.read_u32(table + 4 * i) == cur).expect("current is a state");
    let phase = match m.bus.read(addr + 0x14, 2).unwrap() as u16 as i16 {
        0 => Phase::Enter,
        1 => Phase::Update,
        2 => Phase::Exit,
        3 => Phase::Done,
        p => panic!("phase {p}"),
    };
    (index as usize, phase, m.bus.read(addr + 0x16, 2).unwrap() as u16 as i16)
}

#[test]
fn matches_the_original_on_random_runs() {
    let Some(exe) = common::exe() else { return };
    for &addr in &MACHINES {
        let def = fsm::read_def(&exe.view(), addr).expect("machine reads");
        let table = exe.view().u32(addr).unwrap();
        // Every action the machine can run: hooked in the original to record
        // the call instead of running it.
        let calls: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut actions: Vec<u32> =
            def.states.iter().flat_map(|s| s.enter.iter().chain(&s.update).chain(&s.exit)).copied().collect();
        actions.sort();
        actions.dedup();
        for seed in 1..=20u64 {
            let mut m = Machine::with_exe(&exe);
            for &a in &actions {
                let calls = calls.clone();
                m.hook(a, move |_, _| {
                    calls.borrow_mut().push(a);
                    0
                });
            }
            calls.borrow_mut().clear();
            m.call(FSM_INIT, &[addr]).unwrap();
            let mut port = Fsm::new(&def);
            let mut rng = common::Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            for step in 0..400 {
                if rng.below(3) == 0 {
                    // Post an event the current state handles most of the
                    // time, otherwise any event.
                    let cur = &def.states[port.current];
                    let event = if !cur.transitions.is_empty() && rng.below(4) != 0 {
                        cur.transitions[rng.below(cur.transitions.len() as u32) as usize].0
                    } else {
                        rng.below(400) as i16
                    };
                    m.call(FSM_POST, &[addr, event as u16 as u32]).unwrap();
                    port.post(&def, event);
                } else {
                    let mut port_calls = Vec::new();
                    calls.borrow_mut().clear();
                    let orig_done = m.call(FSM_TICK, &[addr]).unwrap() & 0xff == 2;
                    let port_done = port.tick(&def, |&a, _| port_calls.push(a));
                    assert_eq!(*calls.borrow(), port_calls, "machine {addr:08x} seed {seed} step {step}: actions");
                    assert_eq!(orig_done, port_done, "machine {addr:08x} seed {seed} step {step}: done");
                }
                let orig = original_state(&mut m, addr, table, def.states.len());
                assert_eq!(orig, (port.current, port.phase, port.event), "machine {addr:08x} seed {seed} step {step}");
                if port.phase == Phase::Done {
                    break;
                }
            }
        }
    }
}
