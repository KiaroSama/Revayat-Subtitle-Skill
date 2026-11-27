//! Authored preservation regressions for the v3.0.3 directional-control migration.

use revayat_core::{markup, subtitle_formats, text_pipeline};

type Check = Result<(), String>;

const RLM: &str = "\u{200f}";
const RLE: &str = "\u{202b}";
const LRE: &str = "\u{202a}";
const PDF: &str = "\u{202c}";
const LRI: &str = "\u{2066}";
const RLI: &str = "\u{2067}";
const PDI: &str = "\u{2069}";

fn isolated(text: &str) -> String {
    format!("{RLM}{RLI}{text}{PDI}")
}

fn anchor(text: &str, kind: &str) -> Result<String, String> {
    text_pipeline::anchor_rtl_line(text.into(), Some(kind.into()), Some("rtl".into()))
}

fn assert_repair(source: &str, expected: &str, kind: &str) -> Check {
    let result = anchor(source, kind)?;
    assert_eq!(result, expected, "{kind}: {source:?}");
    markup::validate_bidi(result.clone(), kind.into())?;
    assert_eq!(anchor(&result, kind)?, result, "repair must be idempotent");
    Ok(())
}

#[test]
fn sibling_embeddings_are_not_mistaken_for_one_outer_wrapper() -> Check {
    for kind in ["srt", "ass"] {
        for source in [
            format!("{RLE}سلام{PDF} {RLE}دنیا{PDF}"),
            format!("{RLE}سلام{PDF} {LRE}ABC 12{PDF}"),
            format!("{RLE}{RLM}سلام{PDF} {RLE}دنیا{RLM}{PDF}"),
        ] {
            markup::validate_bidi(source.clone(), kind.into())?;
            let result = markup::rtl(source.clone(), kind.into(), "rtl".into(), false)?;
            markup::validate_bidi(result.clone(), kind.into())?;
            assert!(
                result.contains(&source),
                "valid sibling scopes must survive"
            );
            assert_eq!(
                subtitle_formats::visible(result.clone(), kind.into())?,
                subtitle_formats::visible(source, kind.into())?
            );
            assert_eq!(
                markup::rtl(result.clone(), kind.into(), "rtl".into(), false)?,
                result
            );
        }
    }
    Ok(())
}

#[test]
fn sibling_embeddings_across_style_runs_keep_their_terminators() -> Check {
    for (kind, open, close) in [("ass", r"{\i1}", r"{\i0}"), ("srt", "<i>", "</i>")] {
        let source = format!("{open}{RLE}سلام{PDF}{close} {open}{RLE}دنیا{PDF}{close}");
        let result = markup::rtl(source, kind.into(), "rtl".into(), false)?;
        markup::validate_bidi(result.clone(), kind.into())?;
        assert_eq!(result.matches(RLE).count(), 2);
        assert_eq!(result.matches(PDF).count(), 2);
        assert_eq!(result.matches(open).count(), 2);
        assert_eq!(result.matches(close).count(), 2);
        assert_eq!(
            markup::rtl(result.clone(), kind.into(), "rtl".into(), false)?,
            result
        );
    }
    Ok(())
}

#[test]
fn stray_openers_and_terminators_do_not_erase_a_balanced_inner_isolate() -> Check {
    let body = format!("سلام {LRI}A 12{PDI} B");
    for kind in ["ass", "srt"] {
        for source in [
            format!("{body}{RLE}"),
            format!("{body}{PDI}"),
            format!("{PDF}{body}"),
            format!("{RLE}{body}"),
        ] {
            assert_repair(&source, &isolated(&body), kind)?;
        }
    }
    Ok(())
}

#[test]
fn nested_valid_scopes_survive_an_unclosed_outer_control() -> Check {
    let body = format!("سلام {LRI}A {RLI}فارسی{PDI} B{PDI} تمام");
    for kind in ["ass", "srt"] {
        assert_repair(&format!("{RLE}{body}"), &isolated(&body), kind)?;
    }
    Ok(())
}

#[test]
fn repair_preserves_scopes_crossing_style_boundaries() -> Check {
    for (kind, open, close) in [("ass", r"{\i1}", r"{\i0}"), ("srt", "<i>", "</i>")] {
        let body = format!("سلام {LRI}A {open}12{close}{PDI} B");
        assert_repair(&format!("{body}{RLE}"), &isolated(&body), kind)?;
    }
    Ok(())
}

#[test]
fn repair_tracks_a_cross_line_isolate_from_an_english_only_line() -> Check {
    for (kind, newline) in [("ass", r"\N"), ("srt", "\n"), ("srt", "<br>")] {
        let body = format!("{LRI}A 12{newline}سلام B{PDI}");
        assert_repair(&format!("{body}{PDI}"), &body, kind)?;
    }
    Ok(())
}

#[test]
fn repair_preserves_cross_line_scopes_when_both_lines_contain_persian() -> Check {
    for (kind, newline) in [("ass", r"\N"), ("srt", "\n")] {
        let body = format!("{LRI}سلام A{newline}دنیا B{PDI}");
        assert_repair(&format!("{body}{PDI}"), &body, kind)?;
    }
    Ok(())
}

#[test]
fn directional_controls_in_ass_tags_and_drawing_data_are_not_repaired() -> Check {
    let protected = format!(r"{{\fnLiteral{RLE}{PDI}}}{{\p1}}m 0 0 l 10 10 {RLE}{PDI}{{\p0}}");
    let body = format!("سلام {LRI}A 12{PDI} B");
    assert_repair(
        &format!("{protected}{body}{PDF}"),
        &format!("{protected}{}", isolated(&body)),
        "ass",
    )
}

#[test]
fn pdi_implicitly_closes_embeddings_inside_its_matching_isolate() -> Check {
    let body = format!("سلام {LRI}A {LRE}B{PDI} تمام");
    for kind in ["ass", "srt"] {
        markup::validate_bidi(body.clone(), kind.into())?;
        assert_repair(&body, &isolated(&body), kind)?;
        assert_repair(&format!("{body}{PDF}"), &isolated(&body), kind)?;
    }
    Ok(())
}

#[test]
fn an_unmatched_pdf_cannot_close_an_isolate() -> Check {
    let body = format!("سلام {LRI}A B{PDI} تمام");
    let source = format!("سلام {LRI}A {PDF}B{PDI} تمام");
    for kind in ["ass", "srt"] {
        assert_repair(&source, &isolated(&body), kind)?;
    }
    Ok(())
}

#[test]
fn an_unmatched_pdi_does_not_pop_an_embedding() -> Check {
    let body = format!("سلام {LRE}A B{PDF} تمام");
    let source = format!("سلام {LRE}A {PDI}B{PDF} تمام");
    for kind in ["ass", "srt"] {
        assert_repair(&source, &isolated(&body), kind)?;
    }
    Ok(())
}

#[test]
fn adding_a_wrapper_does_not_reject_valid_maximum_depth_scopes() -> Check {
    for depth in [1, 124, 125] {
        let source = format!("{}سلام{}", LRI.repeat(depth), PDI.repeat(depth));
        markup::validate_bidi(source.clone(), "srt".into())?;
        let result = markup::rtl(source.clone(), "srt".into(), "rtl".into(), false)?;
        markup::validate_bidi(result.clone(), "srt".into())?;
        assert!(result.contains(&source));
        assert_eq!(
            markup::rtl(result.clone(), "srt".into(), "rtl".into(), false)?,
            result
        );
        let anchored = anchor(&source, "srt")?;
        markup::validate_bidi(anchored.clone(), "srt".into())?;
        assert!(anchored.contains(&source));
        assert_eq!(anchor(&anchored, "srt")?, anchored);
    }
    Ok(())
}

#[test]
fn ltr_wrappers_preserve_valid_maximum_depth_scopes() -> Check {
    for kind in ["srt", "ass"] {
        for depth in [1, 124, 125] {
            let source = format!("{}سلام A{}", LRI.repeat(depth), PDI.repeat(depth));
            markup::validate_bidi(source.clone(), kind.into())?;
            let result = markup::rtl(source.clone(), kind.into(), "ltr".into(), false)?;
            markup::validate_bidi(result.clone(), kind.into())?;
            assert!(result.contains(&source), "valid nested scopes must survive");
            assert_eq!(
                subtitle_formats::visible(result.clone(), kind.into())?,
                subtitle_formats::visible(source, kind.into())?
            );
            assert_eq!(
                markup::rtl(result.clone(), kind.into(), "ltr".into(), false)?,
                result
            );
        }
    }
    Ok(())
}

#[test]
fn excessive_balanced_nesting_is_not_silently_deleted_as_stray_controls() -> Check {
    let source = format!("{}سلام{}", LRI.repeat(126), PDI.repeat(126));
    assert!(markup::rtl(source.clone(), "srt".into(), "rtl".into(), false).is_err());
    if let Ok(result) = anchor(&source, "srt") {
        assert_eq!(
            result, source,
            "unsupported balanced scopes must not be erased"
        );
    }
    Ok(())
}
