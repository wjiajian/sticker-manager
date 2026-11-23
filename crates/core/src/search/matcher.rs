use std::{borrow::Cow, collections::HashMap};

use pinyin::ToPinyinMulti;

const LIMIT: usize = 32;
const MISS: u8 = u8::MAX;

fn normalized(text: &str) -> Cow<'_, str> {
    fn fold(c: char) -> char {
        match c {
            '！'..='～' => char::from_u32(c as u32 - 0xfee0)
                .unwrap()
                .to_ascii_lowercase(),
            _ => c.to_ascii_lowercase(),
        }
    }
    if text.chars().any(|c| fold(c) != c) {
        text.chars().map(fold).collect::<String>().into()
    } else {
        text.into()
    }
}

struct Word {
    text: String,
    // Empty for literal-only words.
    chars: Vec<char>,
}

pub(super) struct Query {
    whole: String,
    words: Vec<Word>,
    readings: HashMap<char, Vec<Cow<'static, str>>>,
}

impl Query {
    pub(super) fn new(query: &str) -> Option<Self> {
        let whole = normalized(query.trim()).into_owned();
        if whole.is_empty() {
            return None;
        }
        let words = whole
            .split_whitespace()
            .map(|text| {
                let chars: Vec<char> = text
                    .chars()
                    .take(LIMIT + 1)
                    .map(|c| match c {
                        'ü' | 'Ü' => 'v',
                        _ => c,
                    })
                    .collect();
                let pinyin = chars.len() <= LIMIT
                    && chars.iter().any(|c| c.is_ascii_lowercase())
                    && chars
                        .iter()
                        .all(|c| c.is_ascii_alphanumeric() || c.to_pinyin_multi().is_some());
                Word {
                    text: text.into(),
                    chars: if pinyin { chars } else { Vec::new() },
                }
            })
            .collect();
        Some(Self {
            whole,
            words,
            readings: HashMap::new(),
        })
    }

    pub(super) fn rank(&mut self, fields: [&str; 3]) -> Option<u8> {
        let fields = fields.map(normalized);
        if fields.iter().any(|f| **f == self.whole) {
            return Some(0);
        }
        let multi = self.words.len() > 1;
        if multi && fields.iter().any(|f| f.starts_with(&self.whole)) {
            return Some(3);
        }
        let mut weakest = 0;
        for word in &self.words {
            let mut best = MISS;
            for field in &fields {
                if !multi && field.starts_with(&word.text) {
                    best = best.min(3);
                } else if field.contains(&word.text) {
                    best = best.min(6);
                }
                if multi && best == 6 {
                    break;
                }
                if !word.chars.is_empty() && !field.is_ascii() {
                    best = best.min(pinyin_rank(field, &word.chars, multi, &mut self.readings));
                }
            }
            if best == MISS {
                return None;
            }
            weakest = weakest.max(best);
        }
        Some(weakest)
    }
}

fn readings(c: char) -> Vec<Cow<'static, str>> {
    let mut result = Vec::new();
    for reading in c.to_pinyin_multi().into_iter().flatten() {
        let plain = reading.plain();
        let plain = if plain.contains('ê') {
            Cow::Owned(plain.replace('ê', "e"))
        } else {
            Cow::Borrowed(plain)
        };
        if plain.contains('ü') {
            for alias in ["u", "v"] {
                let alias = Cow::Owned(plain.replace('ü', alias));
                if !result.contains(&alias) {
                    result.push(alias);
                }
            }
        } else if !result.contains(&plain) {
            result.push(plain);
        }
    }
    result
}

// A row memoizes the best rank at each consumed word position. Each field
// character advances one row, so memory stays bounded for long captions.
fn pinyin_rank(
    field: &str,
    word: &[char],
    multi: bool,
    cache: &mut HashMap<char, Vec<Cow<'static, str>>>,
) -> u8 {
    let mut row = [MISS; LIMIT + 1];
    let mut previous_alnum = false;
    let mut best = MISS;
    for (offset, c) in field.char_indices() {
        let alnum = c.is_ascii_alphanumeric();
        let sounds: &[_] = if c.is_ascii() {
            &[]
        } else {
            cache.entry(c).or_insert_with(|| readings(c))
        };
        if !sounds.is_empty() || alnum && !previous_alnum {
            row[0] = if !multi && offset == 0 { 4 } else { 7 };
        }
        previous_alnum = alnum;
        let mut next = [MISS; LIMIT + 1];
        for (pos, &rank) in row[..word.len()]
            .iter()
            .enumerate()
            .filter(|(_, r)| **r != MISS)
        {
            if (alnum || !sounds.is_empty()) && c == word[pos] {
                next[pos + 1] = next[pos + 1].min(rank);
            }
            for sound in sounds {
                let initial = if ["zh", "ch", "sh"]
                    .iter()
                    .any(|prefix| sound.starts_with(prefix))
                {
                    2
                } else {
                    1
                };
                let matched = sound
                    .bytes()
                    .zip(&word[pos..])
                    .take_while(|(b, ch)| char::from(*b) == **ch)
                    .count();
                for len in 1..=matched {
                    let end = pos + len;
                    if len != sound.len() && len > 1 && len != initial && end != word.len() {
                        continue;
                    }
                    // A whole syllable wins even if it is one letter. A final
                    // prefix longer than its initial is full (xiaba); xiab,
                    // zh and a single z of zhang use the abbreviated rank.
                    let abbreviated = len != sound.len() && len <= initial;
                    let candidate = if abbreviated && rank % 3 == 1 {
                        rank + 1
                    } else {
                        rank
                    };
                    next[end] = next[end].min(candidate);
                }
            }
        }
        let rank = next[word.len()];
        best = best.min(if rank <= 5 && offset + c.len_utf8() == field.len() {
            rank - 3
        } else {
            rank
        });
        if word.len() == 1 {
            break;
        }
        row = next;
    }
    best
}
