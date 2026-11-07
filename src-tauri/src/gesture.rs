//! Push-to-talk gesture interpreter, independent of recording state.
//!
//! Fed by the macOS Fn event tap or by the push-to-talk accelerator on any platform.
//! It only turns key timing into intents; the session coordinator decides what they do.

pub const TAP_THRESHOLD_MS: u64 = 500;
pub const DOUBLE_TAP_WINDOW_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureAction {
    /// Key went down: start a push-to-talk recording.
    StartHold,
    /// A valid hold was released: finish and deliver.
    FinishHold,
    /// A solitary short tap: cancel delivery, keep nothing meaningful.
    DiscardTap,
    /// Second short tap: latch hands-free recording.
    StartHandsFree,
    /// Fn+Space: start or toggle hands-free.
    ToggleHandsFree,
    /// Key tapped while hands-free is active: finish.
    Finish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Holding { down_at: u64 },
    Armed { up_at: u64 },
    /// The press that latched hands-free; its release must not finish the session.
    LatchPress,
    /// The key was used as a chord modifier (Fn+Space); its release does nothing.
    Chord,
    /// Key pressed while hands-free was running; release finishes.
    FinishPress { down_at: u64 },
}

#[derive(Debug)]
pub struct Gesture {
    state: State,
}

impl Default for Gesture {
    fn default() -> Self {
        Self { state: State::Idle }
    }
}

impl Gesture {
    /// Key down. `hands_free_active` reports whether the coordinator is in a latched session.
    pub fn key_down(&mut self, now_ms: u64, hands_free_active: bool) -> Option<GestureAction> {
        match self.state {
            // Key repeat or duplicate down: ignore.
            State::Holding { .. } | State::LatchPress | State::Chord | State::FinishPress { .. } => None,
            _ if hands_free_active => {
                self.state = State::FinishPress { down_at: now_ms };
                None
            }
            State::Armed { up_at } if now_ms.saturating_sub(up_at) <= DOUBLE_TAP_WINDOW_MS => {
                self.state = State::LatchPress;
                Some(GestureAction::StartHandsFree)
            }
            State::Idle | State::Armed { .. } => {
                self.state = State::Holding { down_at: now_ms };
                Some(GestureAction::StartHold)
            }
        }
    }

    pub fn key_up(&mut self, now_ms: u64) -> Option<GestureAction> {
        let (next, action) = match self.state {
            State::Holding { down_at } if now_ms.saturating_sub(down_at) < TAP_THRESHOLD_MS => {
                (State::Armed { up_at: now_ms }, Some(GestureAction::DiscardTap))
            }
            State::Holding { .. } => (State::Idle, Some(GestureAction::FinishHold)),
            State::FinishPress { .. } => (State::Idle, Some(GestureAction::Finish)),
            State::LatchPress | State::Chord => (State::Idle, None),
            s @ (State::Idle | State::Armed { .. }) => (s, None),
        };
        self.state = next;
        action
    }

    /// Space pressed. Returns `(consume, action)`; Space is consumed only while the
    /// gesture key is held, so ordinary typing is never touched.
    pub fn space_down(&mut self) -> (bool, Option<GestureAction>) {
        match self.state {
            State::Holding { .. } | State::FinishPress { .. } | State::LatchPress => {
                self.state = State::Chord;
                (true, Some(GestureAction::ToggleHandsFree))
            }
            State::Chord => (true, None), // key repeat
            _ => (false, None),
        }
    }

    pub fn is_key_held(&self) -> bool {
        matches!(self.state, State::Holding { .. } | State::LatchPress | State::Chord | State::FinishPress { .. })
    }

    /// A mouse stop or cancel resets the keyboard session so a later key release
    /// cannot stop a newer recording.
    pub fn reset(&mut self) {
        self.state = match self.state {
            // Still physically held: swallow the coming release.
            State::Holding { .. } | State::FinishPress { .. } | State::LatchPress => State::Chord,
            _ => State::Idle,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use GestureAction::*;

    #[test]
    fn long_hold_starts_and_finishes_exactly_once() {
        let mut g = Gesture::default();
        assert_eq!(g.key_down(0, false), Some(StartHold));
        assert_eq!(g.key_down(30, false), None, "key repeat ignored");
        assert_eq!(g.key_up(900), Some(FinishHold));
        assert_eq!(g.key_up(950), None);
    }

    #[test]
    fn short_tap_discards_then_double_tap_latches() {
        let mut g = Gesture::default();
        assert_eq!(g.key_down(0, false), Some(StartHold));
        assert_eq!(g.key_up(120), Some(DiscardTap));
        assert_eq!(g.key_down(400, false), Some(StartHandsFree));
        assert_eq!(g.key_up(480), None, "latching release must not finish");
        // Next tap while hands-free finishes on release.
        assert_eq!(g.key_down(5000, true), None);
        assert_eq!(g.key_up(5100), Some(Finish));
    }

    #[test]
    fn slow_second_tap_is_a_new_hold() {
        let mut g = Gesture::default();
        g.key_down(0, false);
        g.key_up(100);
        assert_eq!(g.key_down(100 + DOUBLE_TAP_WINDOW_MS + 1, false), Some(StartHold));
    }

    #[test]
    fn fn_space_chord_toggles_and_consumes_space_only_when_held() {
        let mut g = Gesture::default();
        assert_eq!(g.space_down(), (false, None), "plain typing untouched");
        g.key_down(0, false);
        assert_eq!(g.space_down(), (true, Some(ToggleHandsFree)));
        assert_eq!(g.space_down(), (true, None), "space repeat consumed");
        assert_eq!(g.key_up(2000), None);
        assert_eq!(g.space_down(), (false, None));
    }

    #[test]
    fn mouse_stop_makes_the_pending_release_inert() {
        let mut g = Gesture::default();
        g.key_down(0, false);
        g.reset(); // user clicked Finish in the bar while still holding
        assert_eq!(g.key_up(2000), None);
        assert_eq!(g.key_down(3000, false), Some(StartHold));
    }
}
