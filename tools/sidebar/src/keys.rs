//! The key table (018's resolution), as a small state machine for `gg` and `za`.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Down,
    Up,
    Top,
    Bottom,
    PageDown,
    PageUp,
    Activate,
    Fold,
    Unfold,
    ToggleFold,
    ToggleScope,
    Search,
    Reply,
    Tell,
    Close,
    OpenUi,
    Menu,
    Escape,
}

#[derive(Debug, Default)]
pub struct KeyTable {
    pending: Option<char>,
}

impl KeyTable {
    pub fn feed(&mut self, key: KeyEvent) -> Option<Command> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let pending = self.pending.take();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (pending, key.code) {
            (Some('g'), KeyCode::Char('g')) => return Some(Command::Top),
            (Some('z'), KeyCode::Char('a')) => return Some(Command::ToggleFold),
            (Some(_), KeyCode::Esc) => return None,
            _ => {}
        }
        Some(match key.code {
            KeyCode::Char('d') if ctrl => Command::PageDown,
            KeyCode::Char('u') if ctrl => Command::PageUp,
            KeyCode::Char('j') | KeyCode::Down => Command::Down,
            KeyCode::Char('k') | KeyCode::Up => Command::Up,
            KeyCode::Char('G') | KeyCode::End => Command::Bottom,
            KeyCode::Home => Command::Top,
            KeyCode::PageDown => Command::PageDown,
            KeyCode::PageUp => Command::PageUp,
            KeyCode::Char(c @ ('g' | 'z')) => {
                self.pending = Some(c);
                return None;
            }
            KeyCode::Enter => Command::Activate,
            KeyCode::Char('h') | KeyCode::Left => Command::Fold,
            KeyCode::Char('l') | KeyCode::Right => Command::Unfold,
            KeyCode::Char('f') => Command::ToggleScope,
            KeyCode::Char('/') => Command::Search,
            KeyCode::Char('r') => Command::Reply,
            KeyCode::Char('t') => Command::Tell,
            KeyCode::Char('c') => Command::Close,
            KeyCode::Char('p') => Command::OpenUi,
            KeyCode::Char('m') => Command::Menu,
            KeyCode::Esc => Command::Escape,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(table: &mut KeyTable, code: KeyCode) -> Option<Command> {
        table.feed(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn every_key_in_018s_table() {
        let mut t = KeyTable::default();
        let single = [
            ('j', Command::Down),
            ('k', Command::Up),
            ('G', Command::Bottom),
            ('h', Command::Fold),
            ('l', Command::Unfold),
            ('f', Command::ToggleScope),
            ('/', Command::Search),
            ('r', Command::Reply),
            ('t', Command::Tell),
            ('c', Command::Close),
            ('p', Command::OpenUi),
            ('m', Command::Menu),
        ];
        for (key, command) in single {
            assert_eq!(press(&mut t, KeyCode::Char(key)), Some(command), "{key}");
        }
        assert_eq!(press(&mut t, KeyCode::Enter), Some(Command::Activate));
        assert_eq!(press(&mut t, KeyCode::Esc), Some(Command::Escape));
        assert_eq!(press(&mut t, KeyCode::Char('g')), None);
        assert_eq!(press(&mut t, KeyCode::Char('g')), Some(Command::Top));
        assert_eq!(press(&mut t, KeyCode::Char('z')), None);
        assert_eq!(press(&mut t, KeyCode::Char('a')), Some(Command::ToggleFold));
    }

    #[test]
    fn a_broken_chord_falls_through_to_the_second_key() {
        let mut t = KeyTable::default();
        assert_eq!(press(&mut t, KeyCode::Char('g')), None);
        assert_eq!(press(&mut t, KeyCode::Char('j')), Some(Command::Down));
        assert_eq!(press(&mut t, KeyCode::Char('z')), None);
        assert_eq!(press(&mut t, KeyCode::Esc), None);
        assert_eq!(press(&mut t, KeyCode::Esc), Some(Command::Escape));
    }

    #[test]
    fn releases_are_ignored() {
        let mut t = KeyTable::default();
        let mut release = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(t.feed(release), None);
    }
}
