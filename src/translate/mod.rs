use std::collections::HashMap;

use serde::Serialize;

use crate::error::QueryError;

/// Parsed stat description file(s): maps stat ids plus values to the
/// human-readable text shown in game ("+25 to maximum Life").
///
/// File format reference: Metadata/StatDescriptions/*.txt, UTF-16LE text of
/// `description` blocks. Each block names 1..n stat ids and holds per-language
/// variant lines: value conditions, a quoted display string with {i}
/// placeholders, and trailing value handlers like `negate 1`.
#[derive(Debug)]
pub struct StatDescriptions {
    descriptions: Vec<Description>,
    /// stat id -> every description block the stat appears in
    by_stat: HashMap<String, Vec<usize>>,
    hidden: std::collections::HashSet<String>,
}

#[derive(Debug)]
struct Description {
    stats: Vec<String>,
    /// language name -> variants; "English" always present
    variants: HashMap<String, Vec<Variant>>,
}

#[derive(Debug, Clone)]
struct Variant {
    conditions: Vec<Condition>,
    text: String,
    handlers: Vec<Handler>,
    /// context markers between the conditions and the string, e.g.
    /// `gem_quality`; tagged variants only apply in special UI contexts and
    /// are excluded from normal selection
    tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Condition {
    min: Option<i64>,
    max: Option<i64>,
    negated_value: Option<i64>,
}

impl Condition {
    fn matches(&self, value: i64) -> bool {
        if let Some(negated) = self.negated_value {
            return value != negated;
        }
        self.min.map_or(true, |bound| value >= bound)
            && self.max.map_or(true, |bound| value <= bound)
    }
}

#[derive(Debug, Clone)]
struct Handler {
    kind: HandlerKind,
    /// 1-based stat index the handler applies to (defaults to 1)
    index: Option<usize>,
}

/// One stat id with its value range; a single value has min == max.
pub struct StatValue {
    pub id: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Translation {
    /// display lines in input-stat order
    pub lines: Vec<String>,
    /// stat ids no description block knows about
    pub unmatched: Vec<String>,
    /// stat ids the game intentionally does not display (no_description)
    pub hidden: Vec<String>,
}

/// One way a piece of display text can be produced: the stat ids behind it
/// with the values recovered from the text's numbers.
#[derive(Debug, Serialize, PartialEq)]
pub struct ReverseMatch {
    pub stats: Vec<ReverseStat>,
    /// the display template that matched, placeholders intact
    pub template: String,
    /// false when a rounding handler had to be inverted, so recovered
    /// values may be off by the rounding error
    pub exact: bool,
}

/// Only what the text actually said is populated: a plain number sets
/// `value`, a `(10-20)` range sets `min`/`max`, the in-game copy format
/// `29(27-32)` sets all three, and a `#` wildcard (or a stat that only
/// appears in the block's conditions) sets none.
#[derive(Debug, Serialize, PartialEq)]
pub struct ReverseStat {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
enum Capture {
    Wildcard,
    Value(f64, f64),
    /// in-game copy format: rolled value plus the tier's roll range
    Roll { value: f64, min: f64, max: f64 },
}

impl StatDescriptions {
    /// Parse a stat description file that contains no include directives.
    pub fn parse(text: &str) -> Result<StatDescriptions, QueryError> {
        Self::parse_with(text, &mut |path: &str| {
            Err(QueryError::internal(format!("unresolved include '{}'", path)))
        })
    }

    /// Parse a stat description file, resolving `include "path"` directives
    /// through the given loader (which returns the decoded file content).
    pub fn parse_with<F>(text: &str, resolve_include: &mut F) -> Result<StatDescriptions, QueryError>
    where
        F: FnMut(&str) -> Result<String, QueryError>,
    {
        let mut result = StatDescriptions {
            descriptions: Vec::new(),
            by_stat: HashMap::new(),
            hidden: std::collections::HashSet::new(),
        };
        result.parse_into(text, resolve_include)?;
        Ok(result)
    }

    fn parse_into<F>(&mut self, text: &str, resolve_include: &mut F) -> Result<(), QueryError>
    where
        F: FnMut(&str) -> Result<String, QueryError>,
    {
        let mut lines = text.lines()
            .map(|line| line.trim_end_matches('\r'))
            .enumerate()
            .peekable();

        while let Some((number, line)) = lines.next() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed == "has_identifiers"
                || trimmed == "no_identifiers"
                || trimmed.starts_with('"') {
                continue;
            }
            if let Some(id) = trimmed.strip_prefix("no_description ") {
                self.hidden.insert(id.trim().to_string());
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("include ") {
                let path = path.trim().trim_matches('"');
                let included = resolve_include(path)?;
                self.parse_into(&included, resolve_include)?;
                continue;
            }
            // handed_description declares two stat lists (main and off hand)
            // that share one set of variant lines
            let stat_line_count = if trimmed == "description" || trimmed.starts_with("description ") {
                Some(1)
            } else if trimmed == "handed_description" || trimmed.starts_with("handed_description ") {
                Some(2)
            } else {
                None
            };
            if let Some(stat_line_count) = stat_line_count {
                let descriptions = parse_description(&mut lines, stat_line_count)
                    .map_err(|message| QueryError::internal(
                        format!("stat descriptions line {}: {}", number + 1, message)))?;
                for description in descriptions {
                    let index = self.descriptions.len();
                    for stat in &description.stats {
                        self.by_stat.entry(stat.clone()).or_default().push(index);
                    }
                    self.descriptions.push(description);
                }
                continue;
            }
            // unknown directives shouldn't make the whole file unusable
            log::warn!("stat descriptions line {}: skipping unexpected '{}'", number + 1, trimmed);
        }
        Ok(())
    }

    pub fn translate(&self, stats: &[StatValue], language: &str) -> Translation {
        let mut lines = Vec::new();
        let mut unmatched = Vec::new();
        let mut hidden = Vec::new();
        let provided: std::collections::HashSet<&str> = stats.iter().map(|s| s.id.as_str()).collect();
        let mut consumed: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for stat in stats {
            if consumed.contains(stat.id.as_str()) {
                continue; // already rendered as part of an earlier stat's block
            }
            if self.hidden.contains(&stat.id) {
                hidden.push(stat.id.clone());
                continue;
            }
            let Some(candidates) = self.by_stat.get(&stat.id) else {
                unmatched.push(stat.id.clone());
                continue;
            };

            // combined check: of all blocks this stat appears in, prefer the
            // one that uses the most of the provided stats (so a hybrid pair
            // beats two single lines), then the tightest fit (fewest empty
            // slots), then the latest definition in the file
            let &index = candidates.iter().max_by_key(|&&index| {
                let block_stats = &self.descriptions[index].stats;
                let overlap = block_stats.iter().filter(|s| provided.contains(s.as_str())).count() as i64;
                let missing = block_stats.len() as i64 - overlap;
                (overlap, -missing, index)
            }).expect("by_stat entries are never empty");

            let description = &self.descriptions[index];
            for slot in &description.stats {
                if provided.contains(slot.as_str()) {
                    consumed.insert(slot.as_str());
                }
            }

            let values: Vec<(f64, f64)> = description.stats.iter()
                .map(|slot| {
                    stats.iter()
                        .find(|s| &s.id == slot)
                        .map(|s| (s.min, s.max))
                        .unwrap_or((0.0, 0.0))
                })
                .collect();

            if let Some(line) = description.render(&values, language) {
                lines.push(line);
            }
        }

        Translation { lines, unmatched, hidden }
    }

    /// Reverse text that may span several display lines, as pasted from the
    /// in-game item copy. The whole text is tried first (some templates are
    /// multi-line); otherwise each trimmed line is reversed on its own.
    pub fn reverse_text(&self, text: &str, language: &str) -> Vec<(String, Vec<ReverseMatch>)> {
        let normalized = text.replace("\r\n", "\n");
        let whole = normalized.trim();
        if whole.contains('\n') {
            let matches = self.reverse(whole, language);
            if !matches.is_empty() {
                return vec![(whole.to_string(), matches)];
            }
            return normalized.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| (line.to_string(), self.reverse(line, language)))
                .collect();
        }
        vec![(whole.to_string(), self.reverse(whole, language))]
    }

    /// The inverse of translate: find every stat combination that can produce
    /// the given display text, recovering values from its numbers. Ranges
    /// `(10-20)`, the item copy roll format `29(27-32)`, and `#` wildcards
    /// are accepted where the text has a number.
    pub fn reverse(&self, text: &str, language: &str) -> Vec<ReverseMatch> {
        let mut matches = Vec::new();
        for description in &self.descriptions {
            let Some(variants) = description.variants.get(language)
                .or_else(|| description.variants.get("English")) else { continue };

            for variant in variants {
                if !variant.tags.is_empty() {
                    continue;
                }
                let Some(captures) = match_template(&variant.text, text) else { continue };

                let mut exact = true;
                let mut invert = |slot: usize, displayed: f64| {
                    let (value, inversion_exact) = variant.invert_handlers(slot, displayed);
                    exact &= inversion_exact;
                    value
                };
                // per slot: (rolled value, roll range), each only when the
                // text actually said so
                let recovered: Vec<(Option<f64>, Option<(f64, f64)>)> = (0..description.stats.len())
                    .map(|i| match captures.get(&i) {
                        Some(Capture::Value(min, max)) if min == max => {
                            (Some(invert(i, *min)), None)
                        }
                        Some(Capture::Value(min, max)) => {
                            let (min, max) = (invert(i, *min), invert(i, *max));
                            (None, Some((f64::min(min, max), f64::max(min, max))))
                        }
                        Some(Capture::Roll { value, min, max }) => {
                            let value = invert(i, *value);
                            let (min, max) = (invert(i, *min), invert(i, *max));
                            (Some(value), Some((f64::min(min, max), f64::max(min, max))))
                        }
                        _ => (None, None),
                    })
                    .collect();

                // the game only renders a variant when its value conditions
                // hold, so text like "0% increased ..." must not match a
                // variant gated on 1|#; wildcards have no value to check
                let conditions_hold = variant.conditions.iter().enumerate().all(|(i, condition)| {
                    let (value, range) = recovered.get(i).copied().unwrap_or((None, None));
                    value.or(range.map(|(min, _)| min))
                        .map_or(true, |checked| condition.matches(checked.round() as i64))
                });
                if !conditions_hold {
                    continue;
                }

                let stats = description.stats.iter().enumerate().map(|(i, id)| {
                    let (value, range) = recovered[i];
                    ReverseStat {
                        id: id.clone(),
                        value,
                        min: range.map(|(min, _)| min),
                        max: range.map(|(_, max)| max),
                    }
                }).collect();

                matches.push(ReverseMatch { stats, template: variant.text.clone(), exact });
            }
        }
        matches
    }
}

impl Description {
    fn render(&self, values: &[(f64, f64)], language: &str) -> Option<String> {
        let variants = self.variants.get(language)
            .or_else(|| self.variants.get("English"))?;

        let variant = variants.iter()
            .filter(|variant| variant.tags.is_empty() && variant.matches(values))
            .max_by_key(|variant| variant.specificity())?;

        let transformed: Vec<(f64, f64)> = values.iter().enumerate()
            .map(|(i, &(min, max))| {
                let min = variant.apply_handlers(i, min);
                let max = variant.apply_handlers(i, max);
                if min <= max { (min, max) } else { (max, min) }
            })
            .collect();

        let line = render_placeholders(&variant.text, &transformed);
        if line.is_empty() { None } else { Some(line) }
    }
}

impl Variant {
    fn matches(&self, values: &[(f64, f64)]) -> bool {
        self.conditions.iter().zip(values).all(|(condition, &(min, _))| {
            condition.matches(min.round() as i64)
        })
    }

    fn specificity(&self) -> i32 {
        self.conditions.iter().map(|condition| {
            if condition.negated_value.is_some() {
                return 3;
            }
            match (condition.min, condition.max) {
                (Some(_), Some(_)) => 3,
                (None, None) => 1,
                _ => 2,
            }
        }).sum()
    }

    fn apply_handlers(&self, value_index: usize, value: f64) -> f64 {
        let mut value = value;
        for handler in &self.handlers {
            let applies_to = handler.index.unwrap_or(1) - 1;
            if applies_to == value_index {
                value = handler.kind.apply(value);
            }
        }
        value
    }

    /// Undo the handler chain to recover the stored stat value from a
    /// displayed one. The bool is false when a rounding handler makes the
    /// inversion approximate.
    fn invert_handlers(&self, value_index: usize, value: f64) -> (f64, bool) {
        let mut value = value;
        let mut exact = true;
        for handler in self.handlers.iter().rev() {
            let applies_to = handler.index.unwrap_or(1) - 1;
            if applies_to == value_index {
                let (inverted, inversion_exact) = handler.kind.invert(value);
                value = inverted;
                exact &= inversion_exact;
            }
        }
        (value, exact)
    }
}

/// Match input text against a display template, capturing a value, range, or
/// `#` wildcard at each placeholder. Returns None unless the whole input is
/// consumed.
fn match_template(template: &str, input: &str) -> Option<HashMap<usize, Capture>> {
    let mut captures = HashMap::new();
    let mut rest = input;
    for part in template_parts(template) {
        match part {
            TemplatePart::Literal(literal) => {
                rest = rest.strip_prefix(literal.as_str())?;
            }
            TemplatePart::Placeholder { index, .. } => {
                let (capture, remaining) = parse_capture(rest)?;
                captures.insert(index, capture);
                rest = remaining;
            }
        }
    }
    rest.is_empty().then_some(captures)
}

fn parse_capture(input: &str) -> Option<(Capture, &str)> {
    if let Some(rest) = input.strip_prefix("(#-#)") {
        return Some((Capture::Wildcard, rest));
    }
    if let Some(rest) = input.strip_prefix('#') {
        return Some((Capture::Wildcard, rest));
    }

    let (sign, rest) = match input.as_bytes().first() {
        Some(b'+') => (1.0, &input[1..]),
        Some(b'-') => (-1.0, &input[1..]),
        _ => (1.0, input),
    };
    if let Some(rest) = rest.strip_prefix('(') {
        let ((min, max), rest) = parse_range(rest)?;
        return Some((Capture::Value(sign * min, sign * max), rest));
    }
    let (value, rest) = parse_number(rest)?;
    // the in-game item copy format puts the tier's roll range right after
    // the rolled value: 29(27-32)
    if let Some(inner) = rest.strip_prefix('(') {
        if let Some(((min, max), remaining)) = parse_range(inner) {
            return Some((Capture::Roll {
                value: sign * value,
                min: sign * min,
                max: sign * max,
            }, remaining));
        }
    }
    Some((Capture::Value(sign * value, sign * value), rest))
}

/// The inside of a `(min-max)` range, both bounds possibly negative,
/// consuming the closing parenthesis.
fn parse_range(input: &str) -> Option<((f64, f64), &str)> {
    let (min, rest) = parse_number(input)?;
    let rest = rest.strip_prefix('-')?;
    let (max, rest) = parse_number(rest)?;
    let rest = rest.strip_prefix(')')?;
    Some(((min, max), rest))
}

fn parse_number(input: &str) -> Option<(f64, &str)> {
    let (sign, rest) = match input.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, input),
    };
    let digits = rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len());
    if digits == 0 {
        return None;
    }
    rest[..digits].parse().ok().map(|value: f64| (sign * value, &rest[digits..]))
}

fn parse_description<'a, I>(lines: &mut std::iter::Peekable<I>, stat_line_count: usize) -> Result<Vec<Description>, String>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    // `description` puts the count and ids on one line; `handed_description`
    // puts the count on its own line with each hand's id list on the lines
    // after it, so ids are gathered across lines until the count is reached
    let (_, first_line) = lines.next().ok_or("missing stats line after 'description'")?;
    let mut leftover: Vec<String> = first_line.split_whitespace().map(str::to_string).collect();
    let count: usize = leftover.remove(0).parse()
        .map_err(|_| format!("expected stat count, got '{}'", first_line.trim()))?;

    let mut stat_lists: Vec<Vec<String>> = Vec::with_capacity(stat_line_count);
    let mut current = leftover;
    for _ in 0..stat_line_count {
        while current.len() < count {
            let (_, line) = lines.next().ok_or("unexpected end of file in stat id list")?;
            current.extend(line.split_whitespace().map(str::to_string));
        }
        if current.len() != count {
            return Err(format!("expected {} stat ids, got {}", count, current.len()));
        }
        stat_lists.push(std::mem::take(&mut current));
    }

    let mut variants: HashMap<String, Vec<Variant>> = HashMap::new();
    let mut language = "English".to_string();

    loop {
        let Some(&(_, next)) = lines.peek() else { break };
        let trimmed = next.trim();
        // blank lines can appear inside a block (data quirk); a blank line
        // only ends the block when whatever follows is not part of it
        if trimmed.is_empty() {
            lines.next();
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("lang ") {
            lines.next();
            language = name.trim().trim_matches('"').to_string();
            continue;
        }
        let Ok(variant_count) = trimmed.parse::<usize>() else { break };
        lines.next();

        let mut lang_variants = Vec::with_capacity(variant_count);
        for _ in 0..variant_count {
            let (number, line) = lines.next()
                .ok_or("unexpected end of file inside description block")?;
            // the game data contains the odd malformed line; drop the variant
            // rather than refusing to load the whole file
            match parse_variant(line.trim(), count) {
                Ok(variant) => lang_variants.push(variant),
                Err(message) => log::warn!("skipping stat description variant on line {}: {}", number + 1, message),
            }
        }
        variants.entry(language.clone()).or_default().extend(lang_variants);
    }

    if !variants.contains_key("English") {
        return Err("description block without an English section".to_string());
    }
    Ok(stat_lists.into_iter()
        .map(|stats| Description { stats, variants: variants.clone() })
        .collect())
}

fn parse_variant(line: &str, condition_count: usize) -> Result<Variant, String> {
    let mut rest = line;
    let mut conditions = Vec::with_capacity(condition_count);
    for _ in 0..condition_count {
        let rest_trimmed = rest.trim_start();
        let token_end = rest_trimmed.find(char::is_whitespace)
            .ok_or_else(|| format!("missing display string on '{}'", line))?;
        let token = &rest_trimmed[..token_end];
        conditions.push(parse_condition(token)?);
        rest = &rest_trimmed[token_end..];
    }

    let mut rest = rest.trim_start();
    let mut tags = Vec::new();
    while !rest.is_empty() && !rest.starts_with('"') {
        let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        tags.push(rest[..token_end].to_string());
        rest = rest[token_end..].trim_start();
    }
    if !rest.starts_with('"') {
        return Err(format!("expected quoted display string, got '{}'", line));
    }
    let closing = rest[1..].find('"')
        .ok_or_else(|| format!("unterminated display string on '{}'", line))?;
    let text = rest[1..closing + 1].replace("\\n", "\n");
    let mut handlers: Vec<Handler> = Vec::new();

    for token in rest[closing + 2..].split_whitespace() {
        if let Ok(index) = token.parse::<usize>() {
            if let Some(handler) = handlers.last_mut() {
                handler.index = Some(index);
            }
            continue;
        }
        if let Some(Handler { kind: HandlerKind::ReminderString, .. }) = handlers.last() {
            // reminder text key like `reminderstring ReminderTextFreeze`
            if token.starts_with("ReminderText") {
                continue;
            }
        }
        handlers.push(Handler { kind: HandlerKind::from_name(token), index: None });
    }

    Ok(Variant { conditions, text, handlers, tags })
}

fn parse_condition(token: &str) -> Result<Condition, String> {
    if token == "#" {
        return Ok(Condition::default());
    }
    if let Some(value) = token.strip_prefix('!') {
        let value = value.parse()
            .map_err(|_| format!("bad negated condition '{}'", token))?;
        return Ok(Condition { negated_value: Some(value), ..Condition::default() });
    }
    if token.contains('|') {
        // the data contains occasional typos like "1|1|#"; read the first
        // and last segment as the bounds
        let mut segments = token.split('|');
        let min = segments.next().unwrap_or("#");
        let max = segments.next_back().unwrap_or("#");
        let parse_bound = |bound: &str| -> Result<Option<i64>, String> {
            if bound == "#" {
                return Ok(None);
            }
            bound.parse().map(Some).map_err(|_| format!("bad condition '{}'", token))
        };
        return Ok(Condition { min: parse_bound(min)?, max: parse_bound(max)?, negated_value: None });
    }
    let exact = token.parse()
        .map_err(|_| format!("bad condition '{}'", token))?;
    Ok(Condition { min: Some(exact), max: Some(exact), negated_value: None })
}

/// A display string split into literal text and `{i}` / `{i:+d}` / `{}`
/// placeholders, used for rendering and for reverse matching.
#[derive(Debug, Clone)]
enum TemplatePart {
    Literal(String),
    Placeholder { index: usize, signed: bool },
}

fn template_parts(text: &str) -> Vec<TemplatePart> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    let mut sequential = 0usize;

    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let placeholder = after.find('}').and_then(|close| {
            let inner = &after[..close];
            let (index_part, format_part) = match inner.split_once(':') {
                Some((index, format)) => (index, Some(format)),
                None => (inner, None),
            };
            if !index_part.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let index = if index_part.is_empty() {
                let index = sequential;
                sequential += 1;
                index
            } else {
                index_part.parse().ok()?
            };
            Some((close, TemplatePart::Placeholder { index, signed: format_part == Some("+d") }))
        });

        match placeholder {
            Some((close, part)) => {
                literal.push_str(&rest[..open]);
                if !literal.is_empty() {
                    parts.push(TemplatePart::Literal(std::mem::take(&mut literal)));
                }
                parts.push(part);
                rest = &after[close + 1..];
            }
            None => {
                literal.push_str(&rest[..open + 1]);
                rest = &rest[open + 1..];
            }
        }
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(TemplatePart::Literal(literal));
    }
    parts
}

/// Substitute placeholders. A value whose min and max differ renders as a
/// range: `(10-20)`, signed form `+(10-20)`.
fn render_placeholders(text: &str, values: &[(f64, f64)]) -> String {
    let mut output = String::with_capacity(text.len());
    for part in template_parts(text) {
        match part {
            TemplatePart::Literal(literal) => output.push_str(&literal),
            TemplatePart::Placeholder { index, signed } => {
                let (min, max) = values.get(index).copied().unwrap_or((0.0, 0.0));
                output.push_str(&format_value(min, max, signed));
            }
        }
    }
    output
}

fn format_value(min: f64, max: f64, signed: bool) -> String {
    if (min - max).abs() < 1e-9 {
        let mut formatted = format_number(min);
        if signed && min >= 0.0 {
            formatted.insert(0, '+');
        }
        formatted
    } else {
        let range = format!("({}-{})", format_number(min), format_number(max));
        if signed && min >= 0.0 {
            format!("+{}", range)
        } else {
            range
        }
    }
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        format!("{:.2}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum HandlerKind {
    Negate,
    ThirtyPercentOfValue,
    SixtyPercentOfValue,
    DecisecondsToSeconds,
    DivideByThree,
    DivideByFive,
    DivideBySix,
    DivideByTen0dp,
    DivideByTwelve,
    DivideByFifteen0dp,
    DivideByTwo0dp,
    DivideByTwentyThenDouble0dp,
    DivideByOneHundred,
    DivideByOneHundredAndNegate,
    DivideByOneHundred2dp,
    MillisecondsToSeconds,
    MillisecondsToSeconds0dp,
    MillisecondsToSeconds1dp,
    MillisecondsToSeconds2dp,
    MultiplicativeDamageModifier,
    MultiplicativePermyriadDamageModifier,
    MultiplyByFour,
    TimesTwenty,
    OldLeechPercent,
    OldLeechPermyriad,
    PerMinuteToPerSecond0dp,
    PerMinuteToPerSecond1dp,
    PerMinuteToPerSecond2dp,
    ReminderString,
    /// data-dependent or unknown handlers pass the value through unchanged
    Passthrough,
}

impl HandlerKind {
    fn from_name(name: &str) -> HandlerKind {
        use HandlerKind::*;
        match name {
            "negate" => Negate,
            "30%_of_value" => ThirtyPercentOfValue,
            "60%_of_value" => SixtyPercentOfValue,
            "deciseconds_to_seconds" => DecisecondsToSeconds,
            "divide_by_three" => DivideByThree,
            "divide_by_five" => DivideByFive,
            "divide_by_six" => DivideBySix,
            "divide_by_ten_0dp" => DivideByTen0dp,
            "divide_by_twelve" => DivideByTwelve,
            "divide_by_fifteen_0dp" => DivideByFifteen0dp,
            "divide_by_two_0dp" => DivideByTwo0dp,
            "divide_by_twenty_then_double_0dp" => DivideByTwentyThenDouble0dp,
            "divide_by_one_hundred" => DivideByOneHundred,
            "divide_by_one_hundred_and_negate" => DivideByOneHundredAndNegate,
            "divide_by_one_hundred_2dp" | "divide_by_one_hundred_2dp_if_required" => DivideByOneHundred2dp,
            "milliseconds_to_seconds" => MillisecondsToSeconds,
            "milliseconds_to_seconds_0dp" => MillisecondsToSeconds0dp,
            "milliseconds_to_seconds_1dp" => MillisecondsToSeconds1dp,
            "milliseconds_to_seconds_2dp" | "milliseconds_to_seconds_2dp_if_required" => MillisecondsToSeconds2dp,
            "multiplicative_damage_modifier" => MultiplicativeDamageModifier,
            "multiplicative_permyriad_damage_modifier" => MultiplicativePermyriadDamageModifier,
            "multiply_by_four" => MultiplyByFour,
            "times_twenty" => TimesTwenty,
            "old_leech_percent" => OldLeechPercent,
            "old_leech_permyriad" => OldLeechPermyriad,
            "per_minute_to_per_second" | "per_minute_to_per_second_1dp" => PerMinuteToPerSecond1dp,
            "per_minute_to_per_second_0dp" => PerMinuteToPerSecond0dp,
            "per_minute_to_per_second_2dp" | "per_minute_to_per_second_2dp_if_required" => PerMinuteToPerSecond2dp,
            "reminderstring" => ReminderString,
            _ => Passthrough,
        }
    }

    fn apply(&self, v: f64) -> f64 {
        use HandlerKind::*;
        match self {
            Negate => -v,
            ThirtyPercentOfValue => v * 0.3,
            SixtyPercentOfValue => v * 0.6,
            DecisecondsToSeconds => v / 10.0,
            DivideByThree => v / 3.0,
            DivideByFive => v / 5.0,
            DivideBySix => v / 6.0,
            DivideByTen0dp => (v / 10.0).floor(),
            DivideByTwelve => v / 12.0,
            DivideByFifteen0dp => (v / 15.0).floor(),
            DivideByTwo0dp => (v / 2.0).floor(),
            DivideByTwentyThenDouble0dp => (v / 20.0).floor() * 2.0,
            DivideByOneHundred => v / 100.0,
            DivideByOneHundredAndNegate => -v / 100.0,
            DivideByOneHundred2dp => round_dp(v / 100.0, 2),
            MillisecondsToSeconds => v / 1000.0,
            MillisecondsToSeconds0dp => (v / 1000.0).round(),
            MillisecondsToSeconds1dp => round_dp(v / 1000.0, 1),
            MillisecondsToSeconds2dp => round_dp(v / 1000.0, 2),
            MultiplicativeDamageModifier => v + 100.0,
            MultiplicativePermyriadDamageModifier => v / 100.0 + 100.0,
            MultiplyByFour => v * 4.0,
            TimesTwenty => v * 20.0,
            OldLeechPercent => v / 5.0,
            OldLeechPermyriad => v / 500.0,
            PerMinuteToPerSecond0dp => (v / 60.0).round(),
            PerMinuteToPerSecond1dp => round_dp(v / 60.0, 1),
            PerMinuteToPerSecond2dp => round_dp(v / 60.0, 2),
            ReminderString | Passthrough => v,
        }
    }

    /// Inverse of apply(). The bool is false when apply() rounds, making the
    /// recovered value approximate.
    fn invert(&self, v: f64) -> (f64, bool) {
        use HandlerKind::*;
        match self {
            Negate => (-v, true),
            ThirtyPercentOfValue => (v / 0.3, true),
            SixtyPercentOfValue => (v / 0.6, true),
            DecisecondsToSeconds => (v * 10.0, true),
            DivideByThree => (v * 3.0, true),
            DivideByFive => (v * 5.0, true),
            DivideBySix => (v * 6.0, true),
            DivideByTen0dp => (v * 10.0, false),
            DivideByTwelve => (v * 12.0, true),
            DivideByFifteen0dp => (v * 15.0, false),
            DivideByTwo0dp => (v * 2.0, false),
            DivideByTwentyThenDouble0dp => (v * 10.0, false),
            DivideByOneHundred => (v * 100.0, true),
            DivideByOneHundredAndNegate => (-v * 100.0, true),
            DivideByOneHundred2dp => (v * 100.0, false),
            MillisecondsToSeconds => (v * 1000.0, true),
            MillisecondsToSeconds0dp => (v * 1000.0, false),
            MillisecondsToSeconds1dp => (v * 1000.0, false),
            MillisecondsToSeconds2dp => (v * 1000.0, false),
            MultiplicativeDamageModifier => (v - 100.0, true),
            MultiplicativePermyriadDamageModifier => ((v - 100.0) * 100.0, true),
            MultiplyByFour => (v / 4.0, true),
            TimesTwenty => (v / 20.0, true),
            OldLeechPercent => (v * 5.0, true),
            OldLeechPermyriad => (v * 500.0, true),
            PerMinuteToPerSecond0dp => (v * 60.0, false),
            PerMinuteToPerSecond1dp => (v * 60.0, false),
            PerMinuteToPerSecond2dp => (v * 60.0, false),
            ReminderString | Passthrough => (v, true),
        }
    }
}

fn round_dp(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}
