//! Monsters' elemental resistance ranks. Not the client's: the server holds them. The bundled `data/resistances.txt`
//! comes from LandSandBoat's data/ (`python3 -I data/resist_gen.py LSB OUT`): each species' rank_element merged through
//! ecosystem, family and species as LSB merges it, and each monster's zone, shown name, species and own ranks.

use std::collections::HashMap;
use std::sync::OnceLock;

const BUNDLED: &str = include_str!("../../data/resistances.txt");

/// Ranks in the spell records' element order: fire, ice, wind, earth, thunder, water, light, dark. Higher resists more;
/// below 0 is a weakness.
pub type Ranks = [i8; 8];

const ELEMENTS: [&str; 8] = ["fire", "ice", "wind", "earth", "thunder", "water", "light", "dark"];

#[derive(Default)]
struct Data {
    species: HashMap<String, Ranks>,
    /// (zone, shown name with '_' for spaces, lower case) to the ranks.
    monsters: HashMap<(u16, String), Ranks>,
}

fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| parse(BUNDLED))
}

fn parse(text: &str) -> Data {
    let mut d = Data::default();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first() {
            Some(&"s") if f.len() == 10 => {
                let mut r = [0i8; 8];
                for (k, v) in f[2..].iter().enumerate() {
                    r[k] = v.parse().unwrap_or(0);
                }
                d.species.insert(f[1].to_string(), r);
            }
            Some(&"m") if f.len() >= 4 => {
                let (Ok(zone), Some(&base)) = (f[1].parse::<u16>(), d.species.get(f[3])) else { continue };
                let mut r = base;
                for o in &f[4..] {
                    if let Some((e, v)) = o.split_once('=')
                        && let Some(k) = ELEMENTS.iter().position(|x| *x == e)
                    {
                        r[k] = v.parse().unwrap_or(r[k]);
                    }
                }
                d.monsters.insert((zone, f[2].to_ascii_lowercase()), r);
            }
            _ => {}
        }
    }
    d
}

/// A monster's ranks by its zone and name as the server sends it; None when LSB's data has no such monster there.
pub fn of(zone: u16, name: &str) -> Option<Ranks> {
    data().monsters.get(&(zone, name.trim().replace(' ', "_").to_ascii_lowercase())).copied()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled() {
        // West Ronfaure's Wild Rabbit is a rabbit: weakest to thunder, water and dark (-3).
        assert_eq!(super::of(100, "Wild Rabbit"), Some([-2, -1, -2, -1, -3, -3, -1, -3]));
        assert_eq!(super::of(100, "No Such Monster"), None);
    }
}
