//! The reader, case by case (section 14), from the design's cases.

use super::super::kept::Kept;
use super::*;

fn read(chunks: &[(Stream, &[u8])]) -> Kept {
    let mut kept = Kept::default();
    let mut reader = Reader::default();
    for (stream, bytes) in chunks {
        reader.write(&mut kept, *stream, bytes);
    }
    reader.finish(&mut kept);
    kept
}

fn texts(kept: &Kept) -> Vec<(Stream, String)> {
    kept.all().map(|l| (l.stream, l.text.clone())).collect()
}

fn fg(c: Colour) -> Style {
    Style {
        fg: Some(c),
        ..Style::default()
    }
}

#[test]
fn two_streams_never_share_a_row_and_number_in_the_order_they_began() {
    let kept = read(&[
        (
            Stream::Out,
            b"\x1b[4mC:\\src\\a.ts\x1b[24m\n  \x1b[2m12:7\x1b[22m  \x1b[31merr",
        ),
        (Stream::Err, b"warn: x\n"),
        (Stream::Out, b"or\x1b[39m  msg\n"),
    ]);
    assert_eq!(
        texts(&kept),
        [
            (Stream::Out, "C:\\src\\a.ts".to_owned()),
            (Stream::Out, "  12:7  error  msg".to_owned()),
            (Stream::Err, "warn: x".to_owned()),
        ]
    );
    let line = kept.get(1).unwrap();
    assert_eq!(
        line.runs,
        [
            Run {
                from: 2,
                to: 6,
                style: Style {
                    dim: true,
                    ..Style::default()
                }
            },
            Run {
                from: 8,
                to: 13,
                style: fg(Colour::Red)
            },
        ]
    );
    let first = kept.get(0).unwrap();
    assert!(first.runs[0].style.underline);
    assert!(kept.all().all(|l| !l.open));
}

#[test]
fn a_carriage_return_writes_the_line_again() {
    let kept = read(&[
        (Stream::Err, b"Building 1/40\rBuilding 2/40\rBuil"),
        (Stream::Err, b"ding 40/40\ndone\n"),
    ]);
    let all: Vec<String> = kept.all().map(|l| l.text.clone()).collect();
    assert_eq!(all, ["Building 40/40", "done"]);
}

#[test]
fn crlf_is_one_line_end_even_split_across_chunks() {
    let kept = read(&[(Stream::Out, b"a\r"), (Stream::Out, b"\nb\r\n")]);
    let all: Vec<String> = kept.all().map(|l| l.text.clone()).collect();
    assert_eq!(all, ["a", "b"]);
}

#[test]
fn an_escape_split_across_chunks_and_an_osc_title_are_read_whole() {
    let kept = read(&[
        (Stream::Out, b"a\x1b[3"),
        (Stream::Out, b"8;5;196mred\x1b]0;title\x07\x1b[0m!\x1b[2K\n"),
    ]);
    let line = kept.get(0).unwrap();
    assert_eq!(line.text, "ared!");
    assert_eq!(
        line.runs,
        [Run {
            from: 1,
            to: 4,
            style: fg(Colour::Red)
        }]
    );
}

#[test]
fn every_split_point_of_a_coloured_line_reads_the_same() {
    let whole: &[u8] =
        "\x1b[1;32m✓ ok\x1b[0m \x1b]8;;https://x\x1b\\link\x1b]8;;\x1b\\ é\n".as_bytes();
    let want = read(&[(Stream::Out, whole)]);
    for cut in 1..whole.len() {
        let got = read(&[(Stream::Out, &whole[..cut]), (Stream::Out, &whole[cut..])]);
        assert_eq!(
            got.get(0).unwrap().text,
            want.get(0).unwrap().text,
            "cut at {cut}"
        );
        assert_eq!(
            got.get(0).unwrap().runs,
            want.get(0).unwrap().runs,
            "cut at {cut}"
        );
    }
    assert_eq!(want.get(0).unwrap().text, "✓ ok link é");
}

#[test]
fn the_sgr_table() {
    let mut style = Style::default();
    sgr(&mut style, "1;3;4;35;44");
    assert_eq!(
        style,
        Style {
            fg: Some(Colour::Magenta),
            bg: Some(Colour::Blue),
            bold: true,
            italic: true,
            underline: true,
            ..Style::default()
        }
    );
    sgr(&mut style, "22;23;24;39;49");
    assert_eq!(style, Style::default());
    sgr(&mut style, "92");
    assert_eq!(style.fg, Some(Colour::Green));
    sgr(&mut style, "103");
    assert_eq!(style.bg, Some(Colour::Yellow));
    sgr(&mut style, "");
    assert_eq!(style, Style::default());
    sgr(&mut style, "38;2;30;144;255");
    assert_eq!(style.fg, Some(Colour::Blue));
    sgr(&mut style, "38;5;46");
    assert_eq!(style.fg, Some(Colour::Green));
    sgr(&mut style, "48;5;9");
    assert_eq!(style.bg, Some(Colour::Red));
    sgr(&mut style, "38;5;240");
    assert_eq!(style.fg, Some(Colour::Black));
    sgr(&mut style, "38;5;255");
    assert_eq!(style.fg, Some(Colour::White));
    sgr(&mut style, "38;2;200;200;200");
    assert_eq!(style.fg, Some(Colour::White));
    sgr(&mut style, "38;2;255;220;0");
    assert_eq!(style.fg, Some(Colour::Yellow));
    sgr(&mut style, "38:2:0:200:200");
    assert_eq!(style.fg, Some(Colour::Cyan));
    sgr(&mut style, "38;2;180;0;255");
    assert_eq!(style.fg, Some(Colour::Magenta));
}

#[test]
fn inverse_swaps_the_colours_as_drawn() {
    let kept = read(&[(Stream::Out, b"\x1b[31;7mX\x1b[27mY\n")]);
    let line = kept.get(0).unwrap();
    assert_eq!(line.runs[0].style.bg, Some(Colour::Red));
    assert_eq!(line.runs[0].style.fg, None);
    assert_eq!(line.runs[1].style, fg(Colour::Red));
}

#[test]
fn a_cursor_move_a_title_and_a_bell_are_dropped() {
    let kept = read(&[(Stream::Out, b"a\x1b[2;5Hb\x1b]2;t\x1b\\c\x07d\x1b(Be\n")]);
    assert_eq!(kept.get(0).unwrap().text, "abcde");
}

#[test]
fn a_long_line_is_cut_at_its_cap_with_what_was_cut_counted() {
    let mut kept = Kept::default();
    let mut reader = Reader::new(10);
    reader.write(&mut kept, Stream::Out, b"0123456789abcdef\n");
    let line = kept.get(0).unwrap();
    assert_eq!(line.text, "0123456789");
    assert_eq!(line.cut, 6);
}

#[test]
fn invalid_utf8_is_replaced_a_character_at_a_time() {
    let kept = read(&[(Stream::Out, b"a\xffb\xc3\x28c\n")]);
    assert_eq!(kept.get(0).unwrap().text, "a\u{fffd}b\u{fffd}(c");
    // A character split across chunks is whole.
    let kept = read(&[(Stream::Out, b"\xe2\x9c"), (Stream::Out, b"\x93\n")]);
    assert_eq!(kept.get(0).unwrap().text, "✓");
}

#[test]
fn a_backspace_takes_the_last_character() {
    let kept = read(&[(Stream::Out, b"ab\x08c\n")]);
    assert_eq!(kept.get(0).unwrap().text, "ac");
}

#[test]
fn a_half_line_of_each_stream_interleaved() {
    let kept = read(&[
        (Stream::Out, b"out-a "),
        (Stream::Err, b"err-a "),
        (Stream::Out, b"out-b\n"),
        (Stream::Err, b"err-b\n"),
    ]);
    assert_eq!(
        texts(&kept),
        [
            (Stream::Out, "out-a out-b".to_owned()),
            (Stream::Err, "err-a err-b".to_owned())
        ]
    );
}

#[test]
fn positions_count_utf16_units() {
    let kept = read(&[(Stream::Out, "😀\x1b[31mé\x1b[0m\n".as_bytes())]);
    let line = kept.get(0).unwrap();
    assert_eq!(
        line.runs,
        [Run {
            from: 2,
            to: 3,
            style: fg(Colour::Red)
        }]
    );
    assert_eq!(line.len16, 3);
}

#[test]
fn an_unfinished_line_is_sent_open_and_again_when_it_grows() {
    let mut kept = Kept::default();
    let mut reader = Reader::default();
    reader.write(&mut kept, Stream::Out, b"half");
    let (new, _) = kept.take(100, usize::MAX);
    assert!(new[0].open);
    reader.write(&mut kept, Stream::Out, b" more\n");
    let (new, changed) = kept.take(100, usize::MAX);
    assert!(new.is_empty());
    assert_eq!(changed[0].text, "half more");
    assert!(!changed[0].open);
}

#[test]
fn a_stray_escape_does_not_swallow_the_output() {
    let mut data = b"\x1b[".to_vec();
    data.extend(std::iter::repeat_n(b'1', 100));
    data.extend(b"x\n");
    let kept = read(&[(Stream::Out, &data)]);
    assert!(kept.get(0).unwrap().text.ends_with('x'));
}
