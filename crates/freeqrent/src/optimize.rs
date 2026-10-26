//! Minimal-length segmentation: a shortest-path search over characters where
//! the state is the current mode plus how far into its packing group the
//! segment is. That makes every cost an exact bit count, so the result is the
//! true minimum for the version's character-count widths.

use alloc::vec;
use alloc::vec::Vec;

use crate::segment::{Mode, Segment, is_alphanumeric};
use crate::version::Version;

/// DP states. `Numeric(k)` means the segment so far has `k mod 3` digits in
/// its open group; `Alphanumeric(k)` likewise for pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Byte,
    Numeric(u8),
    Alphanumeric(u8),
}

const STATES: [State; 6] = [
    State::Byte,
    State::Numeric(0),
    State::Numeric(1),
    State::Numeric(2),
    State::Alphanumeric(0),
    State::Alphanumeric(1),
];

impl State {
    fn index(self) -> usize {
        match self {
            State::Byte => 0,
            State::Numeric(k) => 1 + k as usize,
            State::Alphanumeric(k) => 4 + k as usize,
        }
    }

    fn mode(self) -> Mode {
        match self {
            State::Byte => Mode::Byte,
            State::Numeric(_) => Mode::Numeric,
            State::Alphanumeric(_) => Mode::Alphanumeric,
        }
    }
}

#[derive(Clone, Copy)]
struct Cell {
    bits: u32,
    from: Step,
}

/// How a cell was reached; indices point into the previous char's row.
#[derive(Clone, Copy)]
enum Step {
    Unreached,
    Start,
    Continue(usize),
    Switch(usize),
}

const UNREACHED: Cell = Cell {
    bits: u32::MAX,
    from: Step::Unreached,
};

impl Segment {
    /// The shortest sequence of numeric, alphanumeric, and byte segments for
    /// `text` at `version` (character-count widths change at versions 10
    /// and 27). Byte segments hold UTF-8. Empty text gives no segments.
    pub fn optimal(text: &str, version: Version) -> Vec<Segment> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        if chars.is_empty() {
            return Vec::new();
        }
        let header = |mode: Mode| 4 + mode.char_count_bits(version) as u32;

        let mut dp = vec![[UNREACHED; 6]; chars.len()];
        for (i, &(_, c)) in chars.iter().enumerate() {
            // Cheapest state after the previous char, for opening a segment.
            let prev = (i > 0).then(|| dp[i - 1]);
            let open_from = |mode: Mode| -> (u32, Step) {
                match prev {
                    None => (0, Step::Start),
                    Some(row) => STATES
                        .iter()
                        .filter(|s| s.mode() != mode)
                        .map(|s| (row[s.index()].bits, Step::Switch(s.index())))
                        .min_by_key(|&(bits, _)| bits)
                        .expect("other modes exist"),
                }
            };
            let continue_from = |s: State| -> Option<(u32, Step)> {
                let row = prev?;
                let cell = row[s.index()];
                (cell.bits != u32::MAX).then_some((cell.bits, Step::Continue(s.index())))
            };

            let mut row = [UNREACHED; 6];
            let mut offer = |to: State, base: Option<(u32, Step)>, cost: u32| {
                if let Some((bits, from)) = base
                    && bits != u32::MAX
                    && bits + cost < row[to.index()].bits
                {
                    row[to.index()] = Cell {
                        bits: bits + cost,
                        from,
                    };
                }
            };

            // Continuations first so they win ties against a fresh segment.
            let byte_cost = 8 * c.len_utf8() as u32;
            offer(State::Byte, continue_from(State::Byte), byte_cost);
            offer(
                State::Byte,
                Some(open_from(Mode::Byte)),
                header(Mode::Byte) + byte_cost,
            );
            if c.is_ascii_digit() {
                offer(State::Numeric(1), continue_from(State::Numeric(0)), 4);
                offer(State::Numeric(2), continue_from(State::Numeric(1)), 3);
                offer(State::Numeric(0), continue_from(State::Numeric(2)), 3);
                offer(
                    State::Numeric(1),
                    Some(open_from(Mode::Numeric)),
                    header(Mode::Numeric) + 4,
                );
            }
            if c.is_ascii() && is_alphanumeric(c as u8) {
                offer(
                    State::Alphanumeric(1),
                    continue_from(State::Alphanumeric(0)),
                    6,
                );
                offer(
                    State::Alphanumeric(0),
                    continue_from(State::Alphanumeric(1)),
                    5,
                );
                offer(
                    State::Alphanumeric(1),
                    Some(open_from(Mode::Alphanumeric)),
                    header(Mode::Alphanumeric) + 6,
                );
            }
            dp[i] = row;
        }

        // Walk back from the cheapest final state, marking where segments open.
        let last = dp.len() - 1;
        let mut state = (0..6)
            .min_by_key(|&s| dp[last][s].bits)
            .expect("six states");
        let mut opens: Vec<(usize, Mode)> = Vec::new();
        for i in (0..chars.len()).rev() {
            match dp[i][state].from {
                Step::Continue(prev) => state = prev,
                Step::Switch(prev) => {
                    opens.push((i, STATES[state].mode()));
                    state = prev;
                }
                Step::Start => opens.push((i, STATES[state].mode())),
                Step::Unreached => unreachable!("byte mode reaches every char"),
            }
        }
        opens.reverse();

        let mut segments = Vec::with_capacity(opens.len());
        for (k, &(start, mode)) in opens.iter().enumerate() {
            let from = chars[start].0;
            let to = opens
                .get(k + 1)
                .map_or(text.len(), |&(next, _)| chars[next].0);
            let part = &text[from..to];
            segments.push(match mode {
                Mode::Numeric => Segment::numeric(part).expect("digits only"),
                Mode::Alphanumeric => Segment::alphanumeric(part).expect("alphanumeric only"),
                _ => Segment::bytes(part.as_bytes()),
            });
        }
        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::total_bits;

    fn v(n: u8) -> Version {
        Version::new(n).unwrap()
    }

    fn modes(text: &str, version: Version) -> Vec<Mode> {
        Segment::optimal(text, version)
            .iter()
            .map(Segment::mode)
            .collect()
    }

    /// Exhaustive minimum over every split of `text` into runs, each with
    /// every valid mode. Memoized on (offset, previous mode); a run's cost
    /// depends only on the run, so this is still exact.
    fn brute_force_bits(text: &str, version: Version) -> usize {
        fn go(
            text: &str,
            at: usize,
            version: Version,
            prev: Option<Mode>,
            memo: &mut alloc::collections::BTreeMap<(usize, Option<u8>), usize>,
        ) -> usize {
            if at == text.len() {
                return 0;
            }
            let key = (at, prev.map(|m| m as u8));
            if let Some(&known) = memo.get(&key) {
                return known;
            }
            let rest = &text[at..];
            let mut best = usize::MAX;
            for end in rest.char_indices().map(|(i, c)| i + c.len_utf8()) {
                let part = &rest[..end];
                let candidates = [
                    Segment::numeric(part).ok(),
                    Segment::alphanumeric(part).ok(),
                    Some(Segment::bytes(part.as_bytes())),
                ];
                for seg in candidates.into_iter().flatten() {
                    if Some(seg.mode()) == prev {
                        continue;
                    }
                    // `MAX` when the rest can't start in any other mode.
                    let tail = go(text, at + end, version, Some(seg.mode()), memo);
                    if tail != usize::MAX {
                        best = best.min(seg.encoded_bits(version).unwrap() + tail);
                    }
                }
            }
            memo.insert(key, best);
            best
        }
        go(
            text,
            0,
            version,
            None,
            &mut alloc::collections::BTreeMap::new(),
        )
    }

    #[test]
    fn single_mode_inputs_stay_single() {
        assert_eq!(modes("0123456789", v(1)), [Mode::Numeric]);
        assert_eq!(modes("HELLO WORLD", v(1)), [Mode::Alphanumeric]);
        assert_eq!(modes("hello", v(1)), [Mode::Byte]);
        assert!(Segment::optimal("", v(1)).is_empty());
    }

    #[test]
    fn long_digit_runs_split_out() {
        let text = "https://example.com/order/12345678901234567890";
        assert_eq!(modes(text, v(1)), [Mode::Byte, Mode::Numeric]);
        let single = total_bits(&[Segment::bytes(text.as_bytes())], v(1)).unwrap();
        let mixed = total_bits(&Segment::optimal(text, v(1)), v(1)).unwrap();
        assert!(mixed < single);
    }

    #[test]
    fn short_digit_runs_stay_in_bytes() {
        assert_eq!(modes("abc12def", v(1)), [Mode::Byte]);
    }

    #[test]
    fn segments_cover_every_byte() {
        // Round-trip tests check the content; here, that nothing is lost.
        let text = "Ünïcode 12345678 ABCDEFGHIJ xyz 99";
        let segments = Segment::optimal(text, v(5));
        assert!(segments.len() > 1);
        assert_eq!(
            segments.iter().map(Segment::num_chars).sum::<usize>(),
            text.len()
        );
    }

    #[test]
    fn matches_brute_force() {
        let samples = [
            "A1B2C3",
            "0000000000AAAAAAAAAAaaaa",
            "HTTPS://X.CO/123456789",
            "ab123456cd",
            "12345ABCDE12345",
            "1A2B3C4D5E6F7G",
            "é1234567é",
            "  12  ",
        ];
        for text in samples {
            for version in [v(1), v(10), v(27)] {
                let found = total_bits(&Segment::optimal(text, version), version).unwrap();
                assert_eq!(
                    found,
                    brute_force_bits(text, version),
                    "{text:?} v{}",
                    version.number()
                );
            }
        }
    }

    #[test]
    fn matches_brute_force_on_random_text() {
        let alphabet: Vec<char> = "0123456789ABCXYZ $:abcxyzé€".chars().collect();
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..300 {
            let len = (next() % 20) as usize + 1;
            let text: alloc::string::String = (0..len)
                .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
                .collect();
            for version in [v(1), v(10), v(27)] {
                let found = total_bits(&Segment::optimal(&text, version), version).unwrap();
                assert_eq!(
                    found,
                    brute_force_bits(&text, version),
                    "{text:?} v{}",
                    version.number()
                );
            }
        }
    }
}
