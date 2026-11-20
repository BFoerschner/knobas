//! The keys knobas takes while a site holds the keyboard, and Esc twice
//! (decisions §8 and §9). Pure: the platform hooks turn their native event
//! into a [`Press`], hand it to [`Keyboard::press`] with the time, and act on
//! the [`Claim`].
//!
//! The table is short on purpose. A key knobas takes is a key the site cannot
//! have.

use serde::Serialize;

/// How long the second Esc still counts, in milliseconds. Sent to the page
/// with the site's status, so the bar's cap is down exactly this long
/// (decisions §9, §26 item 4: one constant).
pub const ESC_AGAIN_MS: u32 = 500;

/// A key, as the platform hook names it. Characters are the unshifted
/// character the key types with no modifier, lower case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    F5,
    Char(char),
    /// Any key the table does not name.
    Other,
}

/// One key going down.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Press {
    pub key: Option<Key>,
    /// ⌘ on the Mac, Ctrl on Windows.
    pub primary: bool,
    pub shift: bool,
    /// ⌥ on the Mac, Alt on Windows. Ctrl and Alt together are AltGr on
    /// Windows, which types a character and claims nothing.
    pub alt: bool,
    /// Ctrl on the Mac, the Windows key on Windows.
    pub other: bool,
    /// The system says the key is held and this is its repeat.
    pub repeat: bool,
    /// The event's own time stamp, when the platform has one. On the Mac a
    /// key the page did not use comes back with the same stamp (decisions §21
    /// item 4): the same press, which counts once.
    pub stamp: Option<f64>,
}

/// What a claimed key does, sent to the shell as `site:key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyName {
    Search,
    Settings,
    Timer,
    Reload,
    Back,
    Forward,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Close,
    Bar,
    /// The first Esc: passed to the site, and the second now counts.
    EscArmed,
    /// The second Esc within [`ESC_AGAIN_MS`]: back to where the site was
    /// opened from.
    EscLeave,
    /// ⌘Q on the Mac while a site is alive: quitting asks the sites first.
    Quit,
}

impl KeyName {
    /// The name on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Settings => "settings",
            Self::Timer => "timer",
            Self::Reload => "reload",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
            Self::ZoomReset => "zoom-reset",
            Self::Close => "close",
            Self::Bar => "bar",
            Self::EscArmed => "esc-armed",
            Self::EscLeave => "esc-leave",
            Self::Quit => "quit",
        }
    }
}

/// What to do with a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Claim {
    /// The site's page gets the key.
    pub pass: bool,
    /// What knobas does with it, if anything.
    pub name: Option<KeyName>,
}

impl Claim {
    const PASS: Self = Self {
        pass: true,
        name: None,
    };

    fn take(name: KeyName) -> Self {
        Self {
            pass: false,
            name: Some(name),
        }
    }
}

/// Which platform's conventions a press follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
}

/// The table of decisions §8, for one press with ⌘ (Ctrl) and nothing else
/// held but what the row allows. `None`: not knobas's.
fn chord(press: &Press, platform: Platform) -> Option<KeyName> {
    if press.other || press.alt {
        return None;
    }
    let key = press.key?;
    if !press.primary {
        // F5 alone reloads on Windows, as in every browser there.
        return (platform == Platform::Windows && key == Key::F5 && !press.shift)
            .then_some(KeyName::Reload);
    }
    let Key::Char(ch) = key else {
        return None;
    };
    // Shift is part of only one row: ⌘+ is ⌘⇧= on most boards.
    if press.shift {
        return (ch == '+' || ch == '=').then_some(KeyName::ZoomIn);
    }
    Some(match ch {
        'k' => KeyName::Search,
        ',' => KeyName::Settings,
        't' => KeyName::Timer,
        'r' => KeyName::Reload,
        '[' => KeyName::Back,
        ']' => KeyName::Forward,
        '+' | '=' => KeyName::ZoomIn,
        '-' => KeyName::ZoomOut,
        '0' => KeyName::ZoomReset,
        'w' => KeyName::Close,
        'l' => KeyName::Bar,
        'q' if platform == Platform::Mac => KeyName::Quit,
        _ => return None,
    })
}

/// One site's keyboard: the table and Esc's timing.
#[derive(Clone, Debug)]
pub struct Keyboard {
    platform: Platform,
    /// When the first Esc went down, while the second still counts.
    armed_at: Option<f64>,
    /// The stamp of the last press seen, to know a press handed back.
    last_stamp: Option<f64>,
}

impl Keyboard {
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            armed_at: None,
            last_stamp: None,
        }
    }

    /// Whether a first Esc is down and the second still counts at `now`.
    #[must_use]
    pub fn armed(&self, now: f64) -> bool {
        self.armed_at
            .is_some_and(|at| now - at <= f64::from(ESC_AGAIN_MS))
    }

    /// What to do with `press`, which went down at `now` (milliseconds on any
    /// clock that only goes forward). `leave`: Esc has somewhere to go, the
    /// page the site was opened from. With nowhere to go Esc is the site's
    /// alone and arms nothing.
    pub fn press(&mut self, press: &Press, now: f64, leave: bool) -> Claim {
        if let Some(stamp) = press.stamp {
            if self.last_stamp == Some(stamp) {
                // The same press, handed back by the page: it was counted.
                return Claim::PASS;
            }
            self.last_stamp = Some(stamp);
        }
        if press.repeat {
            // A held key counts once. A held chord of knobas's stays knobas's,
            // so the page never sees its repeats either.
            return if chord(press, self.platform).is_some() {
                Claim {
                    pass: false,
                    name: None,
                }
            } else {
                Claim::PASS
            };
        }
        let bare_esc = press.key == Some(Key::Escape)
            && !press.primary
            && !press.shift
            && !press.alt
            && !press.other;
        if !bare_esc {
            self.armed_at = None;
            return chord(press, self.platform).map_or(Claim::PASS, Claim::take);
        }
        if !leave {
            self.armed_at = None;
            return Claim::PASS;
        }
        if self.armed(now) {
            self.armed_at = None;
            return Claim::take(KeyName::EscLeave);
        }
        self.armed_at = Some(now);
        Claim {
            pass: true,
            name: Some(KeyName::EscArmed),
        }
    }

    /// Forget a first Esc: the keyboard went elsewhere.
    pub fn disarm(&mut self) {
        self.armed_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esc() -> Press {
        Press {
            key: Some(Key::Escape),
            ..Press::default()
        }
    }

    fn ch(c: char) -> Press {
        Press {
            key: Some(Key::Char(c)),
            ..Press::default()
        }
    }

    fn cmd(c: char) -> Press {
        Press {
            key: Some(Key::Char(c)),
            primary: true,
            ..Press::default()
        }
    }

    fn claim(press: Press, platform: Platform) -> Claim {
        Keyboard::new(platform).press(&press, 0.0, true)
    }

    const TABLE: &[(char, KeyName)] = &[
        ('k', KeyName::Search),
        (',', KeyName::Settings),
        ('t', KeyName::Timer),
        ('r', KeyName::Reload),
        ('[', KeyName::Back),
        (']', KeyName::Forward),
        ('+', KeyName::ZoomIn),
        ('=', KeyName::ZoomIn),
        ('-', KeyName::ZoomOut),
        ('0', KeyName::ZoomReset),
        ('w', KeyName::Close),
        ('l', KeyName::Bar),
    ];

    #[test]
    fn every_row_of_the_table_is_taken_on_both_platforms() {
        for platform in [Platform::Mac, Platform::Windows] {
            for (c, name) in TABLE {
                assert_eq!(
                    claim(cmd(*c), platform),
                    Claim {
                        pass: false,
                        name: Some(*name)
                    },
                    "{c} on {platform:?}"
                );
            }
        }
    }

    #[test]
    fn f5_reloads_on_windows_only() {
        let f5 = Press {
            key: Some(Key::F5),
            ..Press::default()
        };
        assert_eq!(claim(f5, Platform::Windows).name, Some(KeyName::Reload));
        assert_eq!(claim(f5, Platform::Mac), Claim::PASS);
    }

    #[test]
    fn cmd_q_is_the_macs_alone() {
        assert_eq!(claim(cmd('q'), Platform::Mac).name, Some(KeyName::Quit));
        assert_eq!(claim(cmd('q'), Platform::Windows), Claim::PASS);
    }

    #[test]
    fn everything_else_is_the_sites() {
        for platform in [Platform::Mac, Platform::Windows] {
            for c in ['b', 'a', 'c', 'v', 'x', 'z', 'f', 'p', '/', '1', 'n'] {
                assert_eq!(claim(cmd(c), platform), Claim::PASS, "cmd {c}");
                assert_eq!(claim(ch(c), platform), Claim::PASS, "{c}");
            }
            let other = Press {
                key: Some(Key::Other),
                primary: true,
                ..Press::default()
            };
            assert_eq!(claim(other, platform), Claim::PASS);
            assert_eq!(claim(Press::default(), platform), Claim::PASS);
        }
    }

    #[test]
    fn one_more_modifier_claims_nothing() {
        for platform in [Platform::Mac, Platform::Windows] {
            let shifted = Press {
                shift: true,
                ..cmd('k')
            };
            assert_eq!(claim(shifted, platform), Claim::PASS, "cmd shift k");
            let alt = Press {
                alt: true,
                ..cmd('r')
            };
            assert_eq!(claim(alt, platform), Claim::PASS, "cmd alt r");
            let other = Press {
                other: true,
                ..cmd('w')
            };
            assert_eq!(claim(other, platform), Claim::PASS, "cmd ctrl w");
        }
        // But ⌘⇧= is ⌘+.
        let plus = Press {
            shift: true,
            ..cmd('=')
        };
        assert_eq!(claim(plus, Platform::Mac).name, Some(KeyName::ZoomIn));
    }

    #[test]
    fn altgr_on_windows_claims_nothing() {
        // AltGr is Ctrl and Alt held together: it types `[`, `]`, `{` on a
        // German board, and the page must get them.
        for c in ['[', ']', 'q', 'l', '0'] {
            let altgr = Press {
                alt: true,
                ..cmd(c)
            };
            assert_eq!(claim(altgr, Platform::Windows), Claim::PASS, "{c}");
        }
    }

    // ---- Esc twice

    #[test]
    fn esc_at_0_is_passed_and_arms_and_at_499_leaves() {
        let mut keys = Keyboard::new(Platform::Mac);
        assert_eq!(
            keys.press(&esc(), 1000.0, true),
            Claim {
                pass: true,
                name: Some(KeyName::EscArmed)
            }
        );
        assert!(keys.armed(1499.0));
        assert_eq!(
            keys.press(&esc(), 1499.0, true),
            Claim {
                pass: false,
                name: Some(KeyName::EscLeave)
            }
        );
        assert!(!keys.armed(1499.0));
    }

    #[test]
    fn esc_at_exactly_500_still_counts() {
        let mut keys = Keyboard::new(Platform::Windows);
        keys.press(&esc(), 0.0, true);
        assert_eq!(
            keys.press(&esc(), 500.0, true).name,
            Some(KeyName::EscLeave)
        );
    }

    #[test]
    fn esc_at_501_is_passed_and_arms_again() {
        let mut keys = Keyboard::new(Platform::Mac);
        keys.press(&esc(), 0.0, true);
        assert_eq!(
            keys.press(&esc(), 501.0, true),
            Claim {
                pass: true,
                name: Some(KeyName::EscArmed)
            }
        );
        assert_eq!(
            keys.press(&esc(), 900.0, true).name,
            Some(KeyName::EscLeave)
        );
    }

    #[test]
    fn a_key_between_the_two_disarms() {
        let mut keys = Keyboard::new(Platform::Mac);
        assert_eq!(keys.press(&esc(), 0.0, true).name, Some(KeyName::EscArmed));
        assert_eq!(keys.press(&ch('a'), 100.0, true), Claim::PASS);
        assert_eq!(
            keys.press(&esc(), 300.0, true),
            Claim {
                pass: true,
                name: Some(KeyName::EscArmed)
            }
        );
    }

    #[test]
    fn an_esc_with_a_modifier_is_no_esc() {
        let mut keys = Keyboard::new(Platform::Windows);
        keys.press(&esc(), 0.0, true);
        for press in [
            Press {
                shift: true,
                ..esc()
            },
            Press {
                primary: true,
                ..esc()
            },
            Press { alt: true, ..esc() },
        ] {
            assert_eq!(keys.press(&press, 10.0, true), Claim::PASS);
        }
        // And it disarmed: the next bare Esc arms.
        assert_eq!(keys.press(&esc(), 20.0, true).name, Some(KeyName::EscArmed));
    }

    #[test]
    fn with_nowhere_to_go_esc_is_the_sites_alone() {
        let mut keys = Keyboard::new(Platform::Mac);
        assert_eq!(keys.press(&esc(), 0.0, false), Claim::PASS);
        assert_eq!(keys.press(&esc(), 100.0, false), Claim::PASS);
    }

    #[test]
    fn a_press_handed_back_with_the_same_stamp_counts_once() {
        // The Mac's monitor sees an Esc the page did not use twice, 16 ms
        // apart, with one time stamp.
        let mut keys = Keyboard::new(Platform::Mac);
        let first = Press {
            stamp: Some(77.25),
            ..esc()
        };
        assert_eq!(keys.press(&first, 0.0, true).name, Some(KeyName::EscArmed));
        assert_eq!(keys.press(&first, 16.0, true), Claim::PASS);
        assert!(keys.armed(16.0), "the echo neither left nor disarmed");
        let second = Press {
            stamp: Some(77.60),
            ..esc()
        };
        assert_eq!(
            keys.press(&second, 350.0, true).name,
            Some(KeyName::EscLeave)
        );
    }

    #[test]
    fn a_repeat_does_not_count() {
        let mut keys = Keyboard::new(Platform::Windows);
        keys.press(&esc(), 0.0, true);
        let held = Press {
            repeat: true,
            ..esc()
        };
        assert_eq!(keys.press(&held, 100.0, true), Claim::PASS);
        assert_eq!(keys.press(&held, 130.0, true), Claim::PASS);
        assert_eq!(
            keys.press(&esc(), 300.0, true).name,
            Some(KeyName::EscLeave)
        );
        let held_k = Press {
            repeat: true,
            ..cmd('k')
        };
        assert_eq!(
            keys.press(&held_k, 400.0, true),
            Claim {
                pass: false,
                name: None
            }
        );
    }

    #[test]
    fn the_names_on_the_wire() {
        assert_eq!(KeyName::ZoomReset.as_str(), "zoom-reset");
        assert_eq!(KeyName::EscLeave.as_str(), "esc-leave");
        assert_eq!(
            serde_json::to_string(&KeyName::EscArmed).unwrap(),
            "\"esc-armed\""
        );
    }
}
