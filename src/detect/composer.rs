//! Whether an agent's composer holds a draft, read from an ANSI screen.
//!
//! The composer is the body of the bottom prompt box: the rows between the last
//! two horizontal rules, the region manifests call `prompt_box_body`. Faint text
//! there is a placeholder or a suggested prompt, not a draft, and one leading
//! prompt glyph is chrome. Any other visible text means something is typed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Composer {
    Empty,
    Draft,
    /// No prompt box on screen, so nothing can be said about a draft.
    Missing,
}

pub(crate) fn composer_state(ansi_screen: &str) -> Composer {
    let rows: Vec<StyledRow> = ansi_screen.lines().map(StyledRow::parse).collect();
    let rules: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| super::manifest::is_horizontal_rule(&row.plain))
        .map(|(index, _)| index)
        .collect();
    let [.., top, bottom] = rules[..] else {
        return Composer::Missing;
    };
    let body: String = rows[top + 1..bottom]
        .iter()
        .map(|row| row.unfaint.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim_start();
    let body = match body.chars().next() {
        Some(glyph) if !glyph.is_ascii() && !glyph.is_alphanumeric() => &body[glyph.len_utf8()..],
        _ => body,
    };
    if body.trim().is_empty() {
        Composer::Empty
    } else {
        Composer::Draft
    }
}

struct StyledRow {
    plain: String,
    unfaint: String,
}

impl StyledRow {
    fn parse(line: &str) -> Self {
        let mut plain = String::new();
        let mut unfaint = String::new();
        let mut faint = false;
        let mut chars = line.trim_end_matches('\r').chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\x1b' {
                plain.push(ch);
                unfaint.push(if faint { ' ' } else { ch });
                continue;
            }
            match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let mut final_byte = None;
                    for next in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&next) {
                            final_byte = Some(next);
                            break;
                        }
                        params.push(next);
                    }
                    if final_byte == Some('m') {
                        faint = sgr_faint(&params, faint);
                    }
                }
                Some(']') => {
                    // OSC runs to BEL or ST.
                    while let Some(next) = chars.next() {
                        if next == '\x07' || (next == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        Self { plain, unfaint }
    }
}

fn sgr_faint(params: &str, mut faint: bool) -> bool {
    let codes: Vec<&str> = params.split(';').collect();
    let mut index = 0;
    while index < codes.len() {
        // A colon sub-parameter (`38:2:…`) carries its whole colour itself.
        let code = codes[index];
        match code.parse::<u16>().ok() {
            None if code.is_empty() => faint = false,
            Some(0 | 22) => faint = false,
            Some(2) => faint = true,
            Some(38 | 48 | 58) => {
                index += match codes.get(index + 1).copied() {
                    Some("5") => 2,
                    Some("2") => 4,
                    _ => 0,
                };
            }
            _ => {}
        }
        index += 1;
    }
    faint
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "────────────────────────────────────────";

    fn screen(prompt_row: &str) -> String {
        format!("● Which one?\n\n⏳ awaiting your answer\n\n{RULE}\n{prompt_row}\n{RULE}\n  ~/repo ⎇ main\n")
    }

    #[test]
    fn bare_prompt_box_is_empty() {
        assert_eq!(composer_state(&screen("❯\u{a0}")), Composer::Empty);
    }

    #[test]
    fn faint_suggestion_is_empty() {
        assert_eq!(
            composer_state(&screen("❯\u{a0}\x1b[2mcommit this\x1b[0m")),
            Composer::Empty
        );
        assert_eq!(
            composer_state(&screen("❯ \x1b[38;2;80;80;80;2mtry this\x1b[22m")),
            Composer::Empty
        );
    }

    #[test]
    fn typed_text_is_a_draft() {
        assert_eq!(composer_state(&screen("❯ fix the bug")), Composer::Draft);
        assert_eq!(
            composer_state(&screen("❯ \x1b[38;5;2m/commit\x1b[0m")),
            Composer::Draft
        );
    }

    #[test]
    fn faint_reset_by_sgr_zero_shows_typed_text() {
        assert_eq!(
            composer_state(&screen("❯ \x1b[2mhint\x1b[0m typed")),
            Composer::Draft
        );
    }

    #[test]
    fn colour_parameters_are_not_read_as_faint() {
        // 38;5;2 selects palette colour 2; the 2 is not SGR faint.
        assert_eq!(
            composer_state(&screen("❯ \x1b[38;5;2mtyped\x1b[0m")),
            Composer::Draft
        );
    }

    #[test]
    fn multi_row_draft_counts() {
        let screen = format!("{RULE}\n❯ first line\n  second\n{RULE}\n");
        assert_eq!(composer_state(&screen), Composer::Draft);
    }

    #[test]
    fn no_prompt_box_is_missing() {
        assert_eq!(composer_state("just output\n$ "), Composer::Missing);
        assert_eq!(
            composer_state(&format!("{RULE}\nonly one rule\n")),
            Composer::Missing
        );
    }

    #[test]
    fn osc_sequences_are_not_text() {
        assert_eq!(
            composer_state(&screen("❯ \x1b]8;;https://x\x07\x1b]8;;\x07")),
            Composer::Empty
        );
    }
}
