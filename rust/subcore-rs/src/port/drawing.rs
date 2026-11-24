use super::Regex;

pub(super) fn drawing_mode(block: &str, mut drawing: bool) -> Result<bool, String> {
    let Some(body) = block.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
        return Ok(drawing);
    };
    if !body.contains('}') {
        scan(body, 0, &mut drawing)?;
    }
    Ok(drawing)
}

fn scan(mut body: &str, depth: usize, drawing: &mut bool) -> Result<(), String> {
    if depth > 16 {
        return Err("ValueError: ASS transform nesting exceeds 16 levels".into());
    }
    let tags = pattern!(
        Regex,
        r"\A(?:fscx|fscy|fsc|iclip|alpha|xbord|ybord|xshad|yshad|border|blur|bord|shad|move|fade|clip|frx|fry|frz|fr|be|fax|fay|pbo|pos|org|fad|fsp|fn|fs|fe|kf|ko|kt|an|[1-4][ac]|[biuskKqrptac])"
    );
    let digits = pattern!(Regex, r"\A\+?[0-9]+\z");
    let mut count = 0;
    while let Some((_, after_slash)) = body.split_once('\\') {
        body = after_slash;
        let name_start = body.trim_start_matches([' ', '\t']);
        let Some(m) = tags.find(name_start) else {
            continue;
        };
        let name = m.as_str();
        body = &name_start[m.end()..];
        let next_slash = body.find('\\').unwrap_or(body.len());
        let prefix = &body[..next_slash];
        let mut argument = prefix;
        let mut advance = next_slash;
        if let Some(open) = prefix.find('(') {
            let complex = matches!(
                name,
                "t" | "clip" | "iclip" | "pos" | "org" | "move" | "fad" | "fade"
            );
            let contents = &body[open + 1..];
            let mut level = 1usize;
            let end = contents
                .bytes()
                .position(|c| {
                    if c == b')' {
                        level -= 1;
                    } else if complex && c == b'(' {
                        level += 1;
                    }
                    level == 0
                })
                .ok_or("ValueError: Unbalanced ASS override parentheses")?;
            let inside = &contents[..end];
            advance = open + 2 + end;
            if name == "t"
                && let Some(slash) = inside.find('\\')
            {
                let args = inside[..slash]
                    .rsplit_once(',')
                    .map_or("", |(args, _)| args);
                if args
                    .split(',')
                    .filter(|s| !s.trim_matches([' ', '\t']).is_empty())
                    .count()
                    <= 3
                {
                    scan(&inside[slash..], depth + 1, drawing)?;
                }
            }
            argument = inside
                .split(',')
                .map(|s| s.trim_matches([' ', '\t']))
                .find(|s| !s.is_empty())
                .unwrap_or(&prefix[..open]);
        }
        if name == "p" {
            let argument = argument.trim_matches([' ', '\t']);
            if !digits.is_match(argument) {
                return Err("ValueError: ASS drawing mode must be a nonnegative integer".into());
            }
            *drawing = !argument.trim_start_matches(['+', '0']).is_empty();
        }
        count += 1;
        if count > 4096 {
            return Err("ValueError: ASS override block exceeds token budget".into());
        }
        body = &body[advance..];
    }
    Ok(())
}
