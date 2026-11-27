//! Authored regressions for format-aware FSI level checks at the UAX #9 limit.

use revayat_core::{markup, text_pipeline};

type Check = Result<(), String>;

const RLM: &str = "\u{200f}";
const LRM: &str = "\u{200e}";
const RLI: &str = "\u{2067}";
const FSI: &str = "\u{2068}";
const PDI: &str = "\u{2069}";
const LRE: &str = "\u{202a}";
const PDF: &str = "\u{202c}";

fn rtl_boundary(body: &str, depth: usize) -> String {
    // With the proposed RTL wrapper, FSI's parent is 124 at depth 60.
    format!(
        "NASA سلام {}{RLI}{LRE}{FSI}{body}{PDI}{PDF}{PDI}{} پایان",
        LRE.repeat(depth),
        PDF.repeat(depth)
    )
}

fn ltr_boundary(body: &str) -> String {
    // With the proposed LTR wrapper, FSI's parent is 124 after 61 LREs.
    format!(
        "NASA سلام {}{FSI}{body}{PDI}{} پایان",
        LRE.repeat(61),
        PDF.repeat(61)
    )
}

fn isolate(text: &str) -> String {
    format!("{RLM}{RLI}{text}{PDI}")
}

fn anchor(text: &str, kind: &str) -> Result<String, String> {
    text_pipeline::anchor_rtl_line(text.into(), Some(kind.into()), Some("rtl".into()))
}

fn assert_anchor(source: &str, expected: &str, kind: &str) -> Check {
    let actual = anchor(source, kind)?;
    assert_eq!(
        actual, expected,
        "wrong {kind} anchoring for FSI escape fixture"
    );
    markup::validate_bidi(actual.clone(), kind.into())?;
    assert_eq!(
        anchor(&actual, kind)?,
        actual,
        "anchoring must be idempotent"
    );
    Ok(())
}

#[test]
fn ass_hard_space_is_not_a_strong_latin_character_inside_fsi() -> Check {
    for body in [r"\hدنیا!", "\u{00a0}دنیا!"] {
        let source = rtl_boundary(body, 60);
        assert_anchor(&source, &isolate(&source), "ass")?;
    }
    Ok(())
}

#[test]
fn ass_soft_space_is_not_a_strong_latin_character_inside_fsi() -> Check {
    for body in [r"\nدنیا!", " دنیا!"] {
        let source = rtl_boundary(body, 60);
        assert_anchor(&source, &isolate(&source), "ass")?;
    }
    Ok(())
}

#[test]
fn reviewed_ass_hard_spaces_use_visible_fsi_direction_in_both_outer_directions() -> Check {
    for direction in ["rtl", "ltr"] {
        let source = if direction == "rtl" {
            rtl_boundary(r"\hدنیا!", 60)
        } else {
            ltr_boundary(r"\hدنیا!")
        };
        let expected = if direction == "rtl" {
            isolate(&source)
        } else {
            format!("{LRE}{LRM}{source}{LRM}{PDF}")
        };
        let actual = markup::rtl(source, "ass".into(), direction.into(), true)?;
        assert_eq!(actual, expected, "wrong {direction} reviewed ASS wrapper");
        markup::validate_bidi(actual.clone(), "ass".into())?;
        assert_eq!(
            markup::rtl(actual.clone(), "ass".into(), direction.into(), true)?,
            actual
        );
    }
    Ok(())
}

#[test]
fn reviewed_ass_soft_spaces_are_not_split_before_fsi_analysis() -> Check {
    for direction in ["rtl", "ltr"] {
        let source = if direction == "rtl" {
            rtl_boundary(r"\nدنیا!", 60)
        } else {
            ltr_boundary(r"\nدنیا!")
        };
        let expected = if direction == "rtl" {
            isolate(&source)
        } else {
            format!("{LRE}{LRM}{source}{LRM}{PDF}")
        };
        let actual = markup::rtl(source, "ass".into(), direction.into(), true)?;
        assert_eq!(
            actual, expected,
            "wrong {direction} reviewed soft-space wrapper"
        );
        assert_eq!(
            markup::rtl(actual.clone(), "ass".into(), direction.into(), true)?,
            actual
        );
    }
    for (opening, closing) in [
        ('\u{2066}', '\u{2069}'),
        ('\u{2067}', '\u{2069}'),
        ('\u{2068}', '\u{2069}'),
        ('\u{202a}', '\u{202c}'),
        ('\u{202b}', '\u{202c}'),
    ] {
        for (a, b) in [("سلام OVA", "دنیا"), ("سلام", "دنیا OVA")] {
            let source = format!("{opening}{a}\\n{b}{closing}");
            let rtl = |text| markup::rtl(text, "ass".into(), "rtl".into(), false);
            let result = rtl(source.clone())?;
            markup::validate_bidi(result.clone(), "ass".into())?;
            assert_eq!(result.matches(r"\n").count(), 1);
            assert_eq!(
                result.replace(r"\n", " "),
                rtl(source.replace(r"\n", " "))?,
                "default ASS soft space differs from literal space for {opening:?}"
            );
            assert_eq!(rtl(result.clone())?, result);
        }
    }
    Ok(())
}

#[test]
fn ass_fsi_projection_preserves_style_runs_and_raw_escape_bytes() -> Check {
    let source = rtl_boundary(r"{\i1}\hدنیا!{\i0}", 60);
    assert_anchor(&source, &isolate(&source), "ass")?;
    let expected = isolate(&source);
    let actual = markup::rtl(source, "ass".into(), "rtl".into(), true)?;
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn fsi_level_limits_still_reject_a_real_latin_first_character() -> Check {
    for body in ["hدنیا!", r"\hABC!"] {
        let source = rtl_boundary(body, 60);
        assert_anchor(&source, &source, "ass")?;
    }
    let source = rtl_boundary(r"\hدنیا!", 59);
    assert_anchor(&source, &isolate(&source), "ass")
}

#[test]
fn srt_backslash_h_and_n_remain_literal_strong_latin_characters() -> Check {
    for body in [r"\hدنیا!", r"\nدنیا!"] {
        let source = rtl_boundary(body, 60);
        assert_anchor(&source, &source, "srt")?;
        assert_eq!(
            markup::rtl(source.clone(), "srt".into(), "rtl".into(), true)?,
            format!("{RLM}{source}{RLM}")
        );
        let source = ltr_boundary(body);
        assert_eq!(
            markup::rtl(source.clone(), "srt".into(), "ltr".into(), true)?,
            format!("{LRM}{source}{LRM}")
        );
    }
    Ok(())
}

#[test]
fn ass_q2_soft_break_ends_fsi_before_the_later_persian_character() -> Check {
    let source = format!(r"{{\q2}}{}", rtl_boundary(r"\nدنیا!", 60));
    assert_anchor(&source, &source, "ass")
}

#[test]
fn ass_q0_restores_soft_space_semantics_after_q2() -> Check {
    let source = format!(r"{{\q2}}{{\q0}}{}", rtl_boundary(r"\nدنیا!", 60));
    let expected = format!(r"{{\q2}}{{\q0}}{}", isolate(&rtl_boundary(r"\nدنیا!", 60)));
    assert_anchor(&source, &expected, "ass")
}

#[test]
fn drawing_controls_remain_opaque_when_drawing_mode_crosses_a_break() -> Check {
    let drawing = format!(
        r"{{\p1}}m 0 0 l 2 2\Nm 10 10 {}{}{{\p0}}",
        RLI.repeat(63),
        PDI.repeat(63)
    );
    let body = rtl_boundary("\u{00a0}دنیا!", 60);
    let source = format!("{drawing}{body}");
    let expected = format!("{drawing}{}", isolate(&body));
    assert_anchor(&source, &expected, "ass")
}
