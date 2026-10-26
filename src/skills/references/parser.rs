use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Reference {
    DocumentRelative(PathBuf),
    SkillRelative(PathBuf),
}

pub fn local_references(contents: &str) -> BTreeSet<(usize, Reference)> {
    let visible = without_fenced_code(contents);
    let (targets, token_contents) = markdown_targets(&visible);
    let mut references = targets
        .into_iter()
        .filter_map(|(line, value)| {
            markdown_candidate(&value).map(|path| (line, Reference::DocumentRelative(path)))
        })
        .collect::<BTreeSet<_>>();
    references.extend(
        token_contents
            .lines()
            .enumerate()
            .flat_map(|(index, line)| {
                resource_tokens(line).filter_map(move |token| {
                    token_candidate(token).map(|reference| (index + 1, reference))
                })
            }),
    );
    references
}

fn without_fenced_code(contents: &str) -> String {
    let mut visible = String::new();
    let mut fence = None;
    for line in contents.lines() {
        if let Some(marker) = fence_marker(line) {
            match fence {
                None => fence = Some(marker),
                Some((character, length)) if marker.0 == character && marker.1 >= length => {
                    fence = None;
                }
                Some(_) => {}
            }
        } else if fence.is_none() {
            visible.push_str(line);
        }
        visible.push('\n');
    }
    visible
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let mut characters = line.trim_start().chars();
    let character = characters.next()?;
    if !matches!(character, '`' | '~') {
        return None;
    }
    let length = 1 + characters
        .take_while(|candidate| *candidate == character)
        .count();
    (length >= 3).then_some((character, length))
}

fn markdown_targets(contents: &str) -> (Vec<(usize, String)>, String) {
    let mut targets = Vec::new();
    let mut token_contents = String::new();
    let mut text_start = 0;
    let mut search = 0;
    while let Some(offset) = contents[search..].find("](") {
        let start = search + offset + 2;
        let Some((end, target)) = balanced_target(contents, start) else {
            break;
        };
        let line = contents
            .get(..start)
            .unwrap_or_default()
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        targets.push((line, target));
        token_contents.push_str(contents.get(text_start..start).unwrap_or_default());
        token_contents.extend(contents.get(start..end).unwrap_or_default().chars().map(
            |character| {
                if character == '\n' { '\n' } else { ' ' }
            },
        ));
        text_start = end;
        search = end + 1;
    }
    token_contents.push_str(contents.get(text_start..).unwrap_or_default());
    (targets, token_contents)
}

fn balanced_target(contents: &str, start: usize) -> Option<(usize, String)> {
    let mut depth = 1;
    let mut escaped = false;
    for (offset, character) in contents[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' if depth == 1 => {
                let end = start + offset;
                return Some((end, contents[start..end].to_owned()));
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn markdown_candidate(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    let value = if let Some(value) = value.strip_prefix('<') {
        value.split_once('>')?.0
    } else {
        value.split_whitespace().next().unwrap_or(value)
    };
    relative_candidate(value).map(PathBuf::from)
}

fn resource_tokens(contents: &str) -> impl Iterator<Item = &str> {
    contents
        .split_whitespace()
        .filter(|token| !token.contains("]("))
        .map(trim_token)
}

fn trim_token(token: &str) -> &str {
    token
        .trim_matches(|character: char| {
            matches!(
                character,
                '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';'
            )
        })
        .trim_end_matches(['.', ':'])
}

fn token_candidate(value: &str) -> Option<Reference> {
    let value = relative_candidate(value)?;
    let explicit = value.starts_with("./") || value.starts_with("../");
    let resource = ["agents/", "assets/", "evals/", "references/", "scripts/"]
        .iter()
        .any(|prefix| value.starts_with(prefix));
    if resource {
        return Some(Reference::SkillRelative(Path::new(value).to_owned()));
    }
    explicit.then(|| Reference::DocumentRelative(Path::new(value).to_owned()))
}

fn relative_candidate(value: &str) -> Option<&str> {
    let value = value
        .split('#')
        .next()?
        .trim_end_matches(|character: char| {
            matches!(
                character,
                '`' | '\'' | '"' | ')' | ']' | '}' | '>' | ',' | ';' | '.' | ':'
            )
        });
    if value.is_empty()
        || value.starts_with(['/', '~', '#'])
        || value.contains("://")
        || value.starts_with("mailto:")
    {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{Reference, local_references};
    use std::path::PathBuf;

    #[test]
    fn extracts_relative_links_and_resource_tokens() {
        let contents = "[root](guide.md) [nested](references/a(b).md) `scripts/run.sh`";

        assert_eq!(
            local_references(contents).into_iter().collect::<Vec<_>>(),
            vec![
                (1, Reference::DocumentRelative(PathBuf::from("guide.md"))),
                (
                    1,
                    Reference::DocumentRelative(PathBuf::from("references/a(b).md"))
                ),
                (1, Reference::SkillRelative(PathBuf::from("scripts/run.sh")))
            ]
        );
    }

    #[test]
    fn ignores_external_and_fenced_examples() {
        let contents = "[web](https://example.com/a.md) [anchor](#a)\n```sh\nscripts/no.sh\n```";

        assert!(local_references(contents).is_empty());
    }
}
