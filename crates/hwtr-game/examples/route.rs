//! Lists a BLD route's branches and jumps.
fn main() {
    let path = std::env::args().nth(1).expect("BLD file");
    let b = std::fs::read(path).unwrap();
    let line = hwtr_game::line::BestLine::parse(&b).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let mut stack: Vec<u16> = line.starts.to_vec();
    while let Some(start) = stack.pop() {
        let mut at = start;
        while seen.insert(at) {
            let Some((op, len)) = hwtr_game::ai::Op::decode(&line.stream, at) else { break };
            match &op {
                hwtr_game::ai::Op::Branch(c) => {
                    println!(
                        "{at}: branch {:?}",
                        c.iter().map(|(ch, to)| (format!("{:#x}", ch.mask), *to)).collect::<Vec<_>>()
                    );
                    stack.extend(c.iter().map(|(_, to)| *to));
                    break;
                }
                hwtr_game::ai::Op::Jump(to) => {
                    stack.push(*to);
                    break;
                }
                hwtr_game::ai::Op::Air(p) => println!("{at}: air {p}"),
                _ => {}
            }
            at = at.wrapping_add(len);
        }
    }
}
