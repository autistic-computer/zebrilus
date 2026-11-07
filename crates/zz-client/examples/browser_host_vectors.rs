use std::{collections::BTreeMap, error::Error, fs};
use zz_client::browser_state::session_owner;

fn uint(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut out = Vec::new();
    let mut cases = 0_u64;
    uint(&mut out, 0);
    for count in 0_u64..33 {
        let mut sessions = BTreeMap::new();
        for index in 0..count {
            sessions.insert((index % 5, index * 17), index * 37 + 1);
        }
        for step in 0..4 {
            if step == 1 && count != 0 {
                sessions.insert(((count - 1) % 5, (count - 1) * 17), u64::MAX);
            }
            if step == 2 {
                sessions.remove(&(0, 0));
            }
            if step == 3 && count > 1 {
                sessions.remove(&(1, 17));
            }
            for event in [0, 1, 38, 75, (count.saturating_sub(1)) * 37 + 1, u64::MAX] {
                cases += 1;
                uint(&mut out, sessions.len() as u64);
                for (&(pane, tab), &session) in &sessions {
                    uint(&mut out, pane);
                    uint(&mut out, tab);
                    uint(&mut out, session);
                }
                uint(&mut out, event);
                let owner = session_owner(
                    sessions.iter().map(|(&key, &session)| (key, session)),
                    event,
                );
                out.push(u8::from(owner.is_some()));
                if let Some((pane, tab)) = owner {
                    uint(&mut out, pane);
                    uint(&mut out, tab);
                }
            }
        }
    }
    out[..8].copy_from_slice(&cases.to_le_bytes());
    fs::write(std::env::args().nth(1).ok_or("missing output path")?, out)?;
    Ok(())
}
