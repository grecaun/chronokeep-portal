/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::fs;

use super::*;
use crate::database::sqlite::tests::setup_tests;

#[test]
fn beep_ignore_snaps_into_range() {
    assert_eq!(clamp_beep_ignore("0"), 0);
    assert_eq!(clamp_beep_ignore("60"), 60);
    assert_eq!(clamp_beep_ignore("600"), defaults::MAX_BEEP_IGNORE);
    assert_eq!(clamp_beep_ignore("601"), defaults::MAX_BEEP_IGNORE);
    assert_eq!(clamp_beep_ignore("99999999999999999999"), defaults::MAX_BEEP_IGNORE);
    assert_eq!(clamp_beep_ignore("-1"), 0);
    assert_eq!(clamp_beep_ignore("-0.4"), 0);
    assert_eq!(clamp_beep_ignore("599.6"), defaults::MAX_BEEP_IGNORE);
    assert_eq!(clamp_beep_ignore("1e2"), 100);
    assert_eq!(clamp_beep_ignore(" 30 "), 30);
}

#[test]
fn beep_ignore_falls_back_for_non_numbers() {
    for v in ["", "abc", "NaN", "inf", "-inf", "12abc"] {
        assert_eq!(clamp_beep_ignore(v), defaults::DEFAULT_BEEP_IGNORE, "value {v:?}");
    }
}

fn control_with(path: &str, name: &str, value: &str) -> Control {
    let mut sq = setup_tests(path);
    sq.set_setting(&setting::Setting::new(String::from(name), String::from(value))).unwrap();
    let control = Control::new(&mut sq).unwrap();
    drop(sq);
    _ = fs::remove_file(path);
    control
}

#[test]
fn defaults_on_an_empty_database() {
    let path = "./test_control_defaults.sqlite";
    let mut sq = setup_tests(path);
    let control = Control::new(&mut sq).unwrap();
    assert_eq!(control.read_window, defaults::DEFAULT_READ_WINDOW);
    assert_eq!(control.beep_ignore, defaults::DEFAULT_BEEP_IGNORE);
    assert_eq!(control.upload_interval, defaults::DEFAULT_UPLOAD_INTERVAL);
    assert_eq!(control.volume, defaults::DEFAULT_VOLUME);
    assert!(control.name.starts_with("Chrono Portal "));
    drop(sq);
    _ = fs::remove_file(path);
}

// Chronokeep desktop's read window is a free text box, so any number can be
// saved. The portal must still start (Control::new runs at startup and after
// every settings change).
#[test]
fn read_window_too_large_does_not_crash() {
    let c = control_with("./test_control_rw_large.sqlite", SETTING_READ_WINDOW, "300");
    assert_eq!(c.read_window, u8::MAX);
}

#[test]
fn read_window_not_a_number_does_not_crash() {
    let c = control_with("./test_control_rw_text.sqlite", SETTING_READ_WINDOW, "abc");
    assert_eq!(c.read_window, defaults::DEFAULT_READ_WINDOW);
}

#[test]
fn read_window_zero_is_kept_and_negative_snaps_to_zero() {
    // 0 is a real choice: every read is kept.
    let c = control_with("./test_control_rw_zero.sqlite", SETTING_READ_WINDOW, "0");
    assert_eq!(c.read_window, 0);
    let c = control_with("./test_control_rw_neg.sqlite", SETTING_READ_WINDOW, "-5");
    assert_eq!(c.read_window, 0);
    let c = control_with("./test_control_rw_frac.sqlite", SETTING_READ_WINDOW, "20.4");
    assert_eq!(c.read_window, 20);
}

#[test]
fn bad_volume_does_not_crash() {
    let c = control_with("./test_control_vol_text.sqlite", SETTING_VOLUME, "loud");
    assert_eq!(c.volume, defaults::DEFAULT_VOLUME);
    let c = control_with("./test_control_vol_high.sqlite", SETTING_VOLUME, "4");
    assert_eq!(c.volume, 1.0);
    let c = control_with("./test_control_vol_neg.sqlite", SETTING_VOLUME, "-1");
    assert_eq!(c.volume, 0.0);
    let c = control_with("./test_control_vol_nan.sqlite", SETTING_VOLUME, "NaN");
    assert_eq!(c.volume, defaults::DEFAULT_VOLUME);
}

#[test]
fn bad_upload_interval_does_not_crash() {
    // The uploader waits at least a second whatever this is.
    let c = control_with("./test_control_up_neg.sqlite", SETTING_UPLOAD_INTERVAL, "-5");
    assert_eq!(c.upload_interval, 0);
    let c = control_with("./test_control_up_text.sqlite", SETTING_UPLOAD_INTERVAL, "soon");
    assert_eq!(c.upload_interval, defaults::DEFAULT_UPLOAD_INTERVAL);
    let c = control_with("./test_control_up_big.sqlite", SETTING_UPLOAD_INTERVAL, "1e30");
    assert_eq!(c.upload_interval, u32::MAX as u64);
}

#[test]
fn beep_ignore_from_database_snaps() {
    let c = control_with("./test_control_beep_big.sqlite", SETTING_BEEP_IGNORE, "3000");
    assert_eq!(c.beep_ignore, defaults::MAX_BEEP_IGNORE);
}

#[test]
fn play_sound_accepts_any_case() {
    let c = control_with("./test_control_ps.sqlite", SETTING_PLAY_SOUND, "TRUE");
    assert!(c.play_sound);
    let c = control_with("./test_control_ps2.sqlite", SETTING_PLAY_SOUND, "yes");
    assert!(!c.play_sound);
}
