use revayat_core::{
    lookup, markup, runtime,
    subtitle_formats::{LineEnding, SubtitleFormat, document},
};

#[test]
fn keeps_source_precision_while_reporting_floored_centiseconds() -> Result<(), String> {
    let source = "\u{feff}1\r\n00:00:00,019 --> 00:00:00,039\r\nHello\r\n";
    let document = document::parse(source, "srt")?;
    assert_eq!(document.format(), Some(SubtitleFormat::Srt));
    assert_eq!(document.line_ending, LineEnding::CrLf);
    assert!(document.bom);
    let cue = &document.cues[0];
    assert_eq!((cue.start_cs(), cue.end_cs()), (1, 3));
    assert_eq!((cue.start, cue.end), (19, 39));
    assert_eq!(
        document::effective_times(cue.start, cue.end, "ass"),
        Ok((10, 30))
    );
    assert!(document::effective_times(11, 19, "ass").is_err());
    Ok(())
}

#[test]
fn finds_the_bundled_koodak_and_terminology_banks() -> Result<(), String> {
    let fonts = runtime::assets::bundled_fonts_dir().ok_or("missing bundled fonts")?;
    let bytes = std::fs::read(fonts.join("BKOODB.TTF")).map_err(|error| error.to_string())?;
    let metrics = markup::font_metrics(&bytes, 0).map_err(|error| error.to_string())?;
    assert!(metrics.units_per_em > 0);
    assert!(metrics.ascender > metrics.descender);
    assert!(markup::font_metrics(&bytes[..4], 0).is_err());
    let report = lookup::lookup_text("", false)?;
    assert_eq!(report, serde_json::json!({"idioms": [], "glossary": []}));
    Ok(())
}
