use revayat_core::subtitle_formats::{
    build_ass_style_with_margins,
    conversion::{Options, convert},
    document::{DEFAULT_STYLE, ass_template, parse},
};
use serde_json::json;

const STYLE: &str = "Default,B Koodak,110,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1";

#[test]
fn default_style_and_source_bold() -> Result<(), String> {
    assert_eq!(DEFAULT_STYLE, STYLE);
    let srt = parse("1\n00:00:01,000 --> 00:00:02,000\nHello\n", "srt")?;
    let (converted, _) = convert(&srt, &Options::default())?;
    assert_eq!(converted.styles["Default"].join(","), STYLE);
    for (input, output) in [("-1", "-1"), ("1", "-1"), ("true", "-1"), ("0", "0")] {
        let mut ass = ass_template(Some(STYLE), "", 1920, 1080)?;
        let bold = ass
            .style_fields
            .iter()
            .position(|field| field == "bold")
            .unwrap();
        ass.styles.get_mut("Default").unwrap()[bold] = input.into();
        let (converted, _) = convert(&ass, &Options::default())?;
        assert_eq!(converted.styles["Default"][bold], output);
        let options = Options {
            style: Some(STYLE),
            ..Options::default()
        };
        let (converted, _) = convert(&ass, &options)?;
        assert_eq!(converted.styles["Default"][bold], "0");
    }
    Ok(())
}

#[test]
fn custom_margins_must_be_nonnegative() {
    let style = |l, r, v| {
        build_ass_style_with_margins(
            "Default".into(),
            None,
            110,
            None,
            None,
            None,
            None,
            json!(0),
            2,
            l,
            r,
            v,
        )
    };
    assert!(style(3, 4, 5).unwrap().ends_with(",2,2,2,3,4,5,1"));
    for (l, r, v) in [(-1, 4, 5), (3, -1, 5), (3, 4, -1)] {
        assert!(style(l, r, v).is_err());
    }
}
