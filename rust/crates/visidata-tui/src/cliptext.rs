//! Text clipping and display width utilities.
//!
//! Ported from Python `VisiData`'s `cliptext.py`. Handles Unicode wide characters
//! (CJK) and string truncation for terminal display.

use unicode_width::UnicodeWidthChar;

/// Returns the display width of a string, accounting for wide (CJK) characters.
///
/// Each CJK character occupies 2 terminal columns; ASCII characters occupy 1.
#[must_use]
pub fn dispwidth(s: &str) -> usize {
    s.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

/// Clip a string to fit within `max_width` terminal columns.
///
/// If the string fits, returns it unchanged with its display width.
/// If it must be truncated, appends `truncator` (default "…") and clips
/// from the right.
///
/// Returns `(clipped_string, display_width)`.
#[must_use]
pub fn clipstr(s: &str, max_width: Option<usize>, truncator: &str) -> (String, usize) {
    let Some(max_width) = max_width else {
        let w = dispwidth(s);
        return (s.to_owned(), w);
    };

    if max_width == 0 {
        return (String::new(), 0);
    }

    let full_width = dispwidth(s);
    if full_width <= max_width {
        return (s.to_owned(), full_width);
    }

    let trunc_width = dispwidth(truncator);
    if max_width <= trunc_width {
        // Not enough room for even truncator + one char — just return truncator clipped
        let (t, tw) = clip_to_width(truncator, max_width);
        return (t, tw);
    }

    let avail = max_width - trunc_width;
    let (clipped, clipped_w) = clip_to_width(s, avail);
    let result = format!("{clipped}{truncator}");
    (result, clipped_w + trunc_width)
}

/// Clip a string from the start (keep the right side) to fit within `max_width`.
///
/// Returns `(clipped_string, display_width)`.
#[must_use]
pub fn clipstr_start(s: &str, max_width: usize) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }

    let full_width = dispwidth(s);
    if full_width <= max_width {
        return (s.to_owned(), full_width);
    }

    // Walk backwards, accumulating characters that fit
    let chars: Vec<char> = s.chars().collect();
    let mut width = 0;
    let mut start_idx = chars.len();

    for i in (0..chars.len()).rev() {
        let cw = UnicodeWidthChar::width(chars[i]).unwrap_or(0);
        if width + cw > max_width {
            break;
        }
        width += cw;
        start_idx = i;
    }

    let result: String = chars[start_idx..].iter().collect();
    (result, width)
}

/// Clip a string from the middle, keeping both start and end visible.
///
/// Returns `(clipped_string, display_width)`.
#[must_use]
pub fn clipstr_middle(s: &str, max_width: usize, truncator: &str) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }

    let full_width = dispwidth(s);
    if full_width <= max_width {
        return (s.to_owned(), full_width);
    }

    let trunc_width = dispwidth(truncator);
    if max_width <= trunc_width {
        let (t, tw) = clip_to_width(truncator, max_width);
        return (t, tw);
    }

    let avail = max_width - trunc_width;

    // First pass: try front (floor half) and back (ceiling half)
    let front_budget = avail / 2;
    let back_budget = avail - front_budget;

    let (initial_front, initial_front_w) = clip_to_width(s, front_budget);
    // Give back any unused front budget
    let adjusted_back_budget = back_budget + (front_budget - initial_front_w);
    let (back, back_w) = clip_from_end(s, adjusted_back_budget);
    // Give front any unused back budget
    if back_w < back_budget {
        let expanded_front_budget = front_budget + (back_budget - back_w);
        let (expanded_front, expanded_front_w) = clip_to_width(s, expanded_front_budget);
        let result = format!("{expanded_front}{truncator}{back}");
        return (result, expanded_front_w + trunc_width + back_w);
    }

    let result = format!("{initial_front}{truncator}{back}");
    (result, initial_front_w + trunc_width + back_w)
}

/// Clip a string to fit exactly within `max_width` columns (from the left).
fn clip_to_width(s: &str, max_width: usize) -> (String, usize) {
    let mut result = String::new();
    let mut width = 0;

    for c in s.chars() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if width + cw > max_width {
            break;
        }
        result.push(c);
        width += cw;
    }

    (result, width)
}

/// Take characters from the end of a string to fit within `max_width` columns.
fn clip_from_end(s: &str, max_width: usize) -> (String, usize) {
    let chars: Vec<char> = s.chars().collect();
    let mut width = 0;
    let mut start_idx = chars.len();

    for i in (0..chars.len()).rev() {
        let cw = UnicodeWidthChar::width(chars[i]).unwrap_or(0);
        if width + cw > max_width {
            break;
        }
        width += cw;
        start_idx = i;
    }

    let result: String = chars[start_idx..].iter().collect();
    (result, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- dispwidth tests (from Python test_cliptext.py) ---

    #[test]
    fn dispwidth_ascii() {
        assert_eq!(dispwidth("abcdef"), 6);
    }

    #[test]
    fn dispwidth_wide_chars() {
        // 桜=2, space=1, 高=2, 橋=2 → 7
        assert_eq!(dispwidth("桜 高橋"), 7);
    }

    #[test]
    fn dispwidth_empty() {
        assert_eq!(dispwidth(""), 0);
    }

    #[test]
    fn dispwidth_mixed() {
        // 'a'=1, 'で'=2 → 3
        assert_eq!(dispwidth("aで"), 3);
    }

    // --- clipstr tests (from Python test_cliptext.py, default truncator='…') ---

    #[test]
    fn clipstr_fits() {
        assert_eq!(clipstr("b to", Some(4), "…"), ("b to".into(), 4));
    }

    #[test]
    fn clipstr_plenty_of_room() {
        assert_eq!(clipstr("abcde", Some(8), "…"), ("abcde".into(), 5));
    }

    #[test]
    fn clipstr_truncated() {
        assert_eq!(clipstr(" jsonl", Some(5), "…"), (" jso…".into(), 5));
    }

    #[test]
    fn clipstr_wide_char_fits() {
        // 'abcd'=4, 'で'=2 → 6 fits in 6
        assert_eq!(clipstr("abcdで", Some(6), "…"), ("abcdで".into(), 6));
    }

    #[test]
    fn clipstr_wide_char_truncated() {
        // 'abcd'=4, 'で'=2 → 6 > 5, so clip: 'abcd'=4 + '…'=1 = 5
        assert_eq!(clipstr("abcdで", Some(5), "…"), ("abcd…".into(), 5));
    }

    #[test]
    fn clipstr_single_char() {
        assert_eq!(clipstr("a", Some(1), "…"), ("a".into(), 1));
    }

    #[test]
    fn clipstr_truncator_only() {
        // "ab" doesn't fit in 1, truncator "…" takes 1 col
        assert_eq!(clipstr("ab", Some(1), "…"), ("…".into(), 1));
    }

    #[test]
    fn clipstr_truncator_with_two() {
        assert_eq!(clipstr("abc", Some(2), "…"), ("a…".into(), 2));
    }

    #[test]
    fn clipstr_wide_in_1() {
        assert_eq!(clipstr("で", Some(1), "…"), ("…".into(), 1));
    }

    #[test]
    fn clipstr_wide_pair_in_1() {
        assert_eq!(clipstr("でで", Some(1), "…"), ("…".into(), 1));
    }

    #[test]
    fn clipstr_wide_pair_in_2() {
        // "でで"=4, width=2, truncator=1, avail=1, but 'で' needs 2 → empty + "…"
        assert_eq!(clipstr("でで", Some(2), "…"), ("…".into(), 1));
    }

    #[test]
    fn clipstr_wide_pair_in_3() {
        assert_eq!(clipstr("でで", Some(3), "…"), ("で…".into(), 3));
    }

    #[test]
    fn clipstr_wide_triple_in_4() {
        assert_eq!(clipstr("ででで", Some(4), "…"), ("で…".into(), 3));
    }

    #[test]
    fn clipstr_wide_triple_in_5() {
        assert_eq!(clipstr("ででで", Some(5), "…"), ("でで…".into(), 5));
    }

    #[test]
    fn clipstr_empty() {
        assert_eq!(clipstr("", Some(1), "…"), ("".into(), 0));
    }

    #[test]
    fn clipstr_no_limit() {
        assert_eq!(clipstr("abcdef", None, "…"), ("abcdef".into(), 6));
        assert_eq!(clipstr("ででで", None, "…"), ("ででで".into(), 6));
    }

    #[test]
    fn clipstr_wide_100_no_limit() {
        let s: String = "で".repeat(100);
        assert_eq!(clipstr(&s, None, "…"), (s.clone(), 200));
    }

    // --- clipstr with empty truncator (from Python test_cliptext.py) ---

    #[test]
    fn clipstr_empty_truncator_fits() {
        assert_eq!(clipstr("b to", Some(4), ""), ("b to".into(), 4));
    }

    #[test]
    fn clipstr_empty_truncator_clips() {
        assert_eq!(clipstr(" jsonl", Some(5), ""), (" json".into(), 5));
    }

    #[test]
    fn clipstr_empty_truncator_wide() {
        assert_eq!(clipstr("abcdで", Some(5), ""), ("abcd".into(), 4));
    }

    #[test]
    fn clipstr_empty_truncator_single() {
        assert_eq!(clipstr("ab", Some(1), ""), ("a".into(), 1));
    }

    #[test]
    fn clipstr_empty_truncator_wide_single() {
        assert_eq!(clipstr("で", Some(1), ""), ("".into(), 0));
    }

    #[test]
    fn clipstr_empty_truncator_wide_pair() {
        assert_eq!(clipstr("でで", Some(2), ""), ("で".into(), 2));
        assert_eq!(clipstr("でで", Some(3), ""), ("で".into(), 2));
    }

    #[test]
    fn clipstr_empty_truncator_wide_triple() {
        assert_eq!(clipstr("ででで", Some(4), ""), ("でで".into(), 4));
        assert_eq!(clipstr("ででで", Some(5), ""), ("でで".into(), 4));
    }

    // --- clipstr_start tests (from Python test_cliptext.py) ---

    #[test]
    fn clipstr_start_fits() {
        assert_eq!(clipstr_start("b to", 4), ("b to".into(), 4));
    }

    #[test]
    fn clipstr_start_plenty() {
        assert_eq!(clipstr_start("abcde", 8), ("abcde".into(), 5));
    }

    #[test]
    fn clipstr_start_clip() {
        assert_eq!(clipstr_start(" jsonl", 5), ("jsonl".into(), 5));
    }

    #[test]
    fn clipstr_start_wide_fits() {
        assert_eq!(clipstr_start("abcdで", 6), ("abcdで".into(), 6));
    }

    #[test]
    fn clipstr_start_wide_clip() {
        assert_eq!(clipstr_start("abcdで", 5), ("bcdで".into(), 5));
    }

    #[test]
    fn clipstr_start_wide_both_ends() {
        assert_eq!(clipstr_start("でbcdで", 6), ("bcdで".into(), 5));
    }

    #[test]
    fn clipstr_start_wide_long() {
        assert_eq!(clipstr_start("でbcdefghiで", 10), ("bcdefghiで".into(), 10));
    }

    #[test]
    fn clipstr_start_wide_tight() {
        assert_eq!(clipstr_start("でbcdefghiで", 3), ("iで".into(), 3));
    }

    #[test]
    fn clipstr_start_wide_2() {
        assert_eq!(clipstr_start("でbcdで", 2), ("で".into(), 2));
    }

    #[test]
    fn clipstr_start_wide_1() {
        assert_eq!(clipstr_start("でbcdで", 1), ("".into(), 0));
    }

    #[test]
    fn clipstr_start_zero() {
        assert_eq!(clipstr_start("でbcdで", 0), ("".into(), 0));
    }

    // --- clipstr_middle tests (from Python test_cliptext.py) ---

    #[test]
    fn clipstr_middle_ascii() {
        assert_eq!(clipstr_middle("1234567890", 6, "…"), ("12…890".into(), 6));
        assert_eq!(clipstr_middle("1234567890", 7, "…"), ("123…890".into(), 7));
        assert_eq!(clipstr_middle("1234567890", 8, "…"), ("123…7890".into(), 8));
        assert_eq!(
            clipstr_middle("1234567890", 9, "…"),
            ("1234…7890".into(), 9)
        );
        assert_eq!(
            clipstr_middle("1234567890", 10, "…"),
            ("1234567890".into(), 10)
        );
        assert_eq!(
            clipstr_middle("1234567890", 11, "…"),
            ("1234567890".into(), 10)
        );
    }

    #[test]
    fn clipstr_middle_all_wide() {
        let s = "ででででで"; // 10 cols
        assert_eq!(clipstr_middle(s, 0, "…"), ("".into(), 0));
        assert_eq!(clipstr_middle(s, 1, "…"), ("…".into(), 1));
        assert_eq!(clipstr_middle(s, 2, "…"), ("…".into(), 1));
        assert_eq!(clipstr_middle(s, 3, "…"), ("…で".into(), 3));
        assert_eq!(clipstr_middle(s, 4, "…"), ("…で".into(), 3));
        assert_eq!(clipstr_middle(s, 5, "…"), ("で…で".into(), 5));
        assert_eq!(clipstr_middle(s, 10, "…"), ("ででででで".into(), 10));
        assert_eq!(clipstr_middle(s, 11, "…"), ("ででででで".into(), 10));
    }
}
