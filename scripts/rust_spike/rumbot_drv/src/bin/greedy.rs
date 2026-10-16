// Same scripted policy as Jackdaw's runner.greedy_play_agent: select, play first 5, cash out, leave shop.
use rumbot_sim::game::{Action, ActionType, GameState, Phase};
fn main() {
    let n: usize = std::env::args().nth(1).map(|x| x.parse().unwrap()).unwrap_or(2000);
    let t = std::time::Instant::now();
    let mut steps = 0u64;
    for i in 0..n {
        let mut g = GameState::new(format!("PERF_{}", i), "Red Deck", 1);
        while !g.is_over() && steps < u64::MAX {
            let a = match g.phase {
                Phase::BlindSelect => Action::new(ActionType::SelectBlind),
                Phase::Playing => Action::with_cards(ActionType::Play, (0..g.hand.len().min(5)).collect()),
                Phase::RoundEval => Action::new(ActionType::CashOut),
                Phase::Shop => Action::new(ActionType::LeaveShop),
                Phase::Pack => Action::new(ActionType::SkipPack),
                _ => break,
            };
            g.step(&a);
            steps += 1;
        }
    }
    let s = t.elapsed().as_secs_f64();
    println!("rumbot: {} games, {} steps in {:.2}s = {:.0} steps/s, {:.0} games/s", n, steps, s, steps as f64 / s, n as f64 / s);
}
