use poe_query_lib::error::QueryError;
use poe_query_lib::translate::{StatDescriptions, StatValue, Translation};

const FIXTURE: &str = "\
description\r
\t1 base_maximum_life\r
\t1\r
\t\t# \"{0:+d} to maximum Life\"\r
\tlang \"German\"\r
\t1\r
\t\t# \"{0:+d} zu maximalem Leben\"\r
\r
description\r
\t1 attack_speed_+%\r
\t2\r
\t\t#|-1 \"{0}% reduced Attack Speed\" negate 1\r
\t\t1|# \"{0}% increased Attack Speed\"\r
\r
description\r
\t2 spell_minimum_base_fire_damage spell_maximum_base_fire_damage\r
\t1\r
\t\t# # \"Deals {0} to {1} Fire Damage\"\r
\r
description\r
\t1 breach_splinter_conversion_permyriad\r
\t1\r
\t\t1|# \"{0}% chance to drop Breachstones\" divide_by_one_hundred_2dp_if_required 1\r
\r
description\r
\t1 skill_cooldown_ms\r
\t1\r
\t\t# \"{0} second Cooldown\" milliseconds_to_seconds_1dp 1\r
\r
description\r
\t1 exact_zero_stat\r
\t2\r
\t\t0 \"exactly nothing\"\r
\t\t# \"{0} of something\"\r
\r
description\r
\t1 future_handler_stat\r
\t1\r
\t\t# \"{0} mysterious units\" some_future_handler 1\r
\r
description\r
\t1 sockets_chance_+%\r
\t1\r
\t\t1|# \"Items found have {}% chance for maximum Sockets\"\r
\r
no_description internal_bookkeeping_stat\r
";

fn single(id: &str, value: f64) -> StatValue {
    StatValue { id: id.to_string(), min: value, max: value }
}

fn translate(stats: Vec<StatValue>) -> Translation {
    StatDescriptions::parse(FIXTURE).unwrap().translate(&stats, "English")
}

#[test]
fn simple_stat_with_sign() {
    let result = translate(vec![single("base_maximum_life", 25.0)]);
    assert_eq!(result.lines, vec!["+25 to maximum Life"]);
    assert!(result.unmatched.is_empty());
}

#[test]
fn language_selection_and_fallback() {
    let descriptions = StatDescriptions::parse(FIXTURE).unwrap();
    let german = descriptions.translate(&[single("base_maximum_life", 25.0)], "German");
    assert_eq!(german.lines, vec!["+25 zu maximalem Leben"]);

    // attack speed block has no German section: falls back to English
    let fallback = descriptions.translate(&[single("attack_speed_+%", 8.0)], "German");
    assert_eq!(fallback.lines, vec!["8% increased Attack Speed"]);
}

#[test]
fn negate_selects_reduced_wording() {
    let result = translate(vec![single("attack_speed_+%", -8.0)]);
    assert_eq!(result.lines, vec!["8% reduced Attack Speed"]);

    let result = translate(vec![single("attack_speed_+%", 12.0)]);
    assert_eq!(result.lines, vec!["12% increased Attack Speed"]);
}

#[test]
fn two_stat_block_renders_one_line() {
    let result = translate(vec![
        single("spell_minimum_base_fire_damage", 5.0),
        single("spell_maximum_base_fire_damage", 12.0),
    ]);
    assert_eq!(result.lines, vec!["Deals 5 to 12 Fire Damage"]);
}

#[test]
fn value_ranges_render_in_parentheses() {
    let result = translate(vec![StatValue {
        id: "base_maximum_life".to_string(),
        min: 10.0,
        max: 20.0,
    }]);
    assert_eq!(result.lines, vec!["+(10-20) to maximum Life"]);
}

#[test]
fn divide_handler_with_decimals() {
    let result = translate(vec![single("breach_splinter_conversion_permyriad", 25.0)]);
    assert_eq!(result.lines, vec!["0.25% chance to drop Breachstones"]);
}

#[test]
fn milliseconds_handler() {
    let result = translate(vec![single("skill_cooldown_ms", 4500.0)]);
    assert_eq!(result.lines, vec!["4.5 second Cooldown"]);
}

#[test]
fn exact_condition_beats_wildcard() {
    let result = translate(vec![single("exact_zero_stat", 0.0)]);
    assert_eq!(result.lines, vec!["exactly nothing"]);

    let result = translate(vec![single("exact_zero_stat", 3.0)]);
    assert_eq!(result.lines, vec!["3 of something"]);
}

#[test]
fn unknown_handler_passes_value_through() {
    let result = translate(vec![single("future_handler_stat", 7.0)]);
    assert_eq!(result.lines, vec!["7 mysterious units"]);
}

#[test]
fn empty_placeholder_is_sequential() {
    let result = translate(vec![single("sockets_chance_+%", 30.0)]);
    assert_eq!(result.lines, vec!["Items found have 30% chance for maximum Sockets"]);
}

#[test]
fn hidden_and_unmatched_are_reported() {
    let result = translate(vec![
        single("internal_bookkeeping_stat", 1.0),
        single("completely_unknown_stat", 2.0),
    ]);
    assert!(result.lines.is_empty());
    assert_eq!(result.hidden, vec!["internal_bookkeeping_stat"]);
    assert_eq!(result.unmatched, vec!["completely_unknown_stat"]);
}

#[test]
fn include_directives_resolve_through_loader() {
    let main = "include \"Metadata/StatDescriptions/base.txt\"\r\ndescription\r\n\t1 extra_stat\r\n\t1\r\n\t\t# \"{0} extra\"\r\n";
    let included = "description\r\n\t1 included_stat\r\n\t1\r\n\t\t# \"{0} included\"\r\n";

    let descriptions = StatDescriptions::parse_with(main, &mut |path| {
        assert_eq!(path, "Metadata/StatDescriptions/base.txt");
        Ok(included.to_string())
    }).unwrap();

    let result = descriptions.translate(&[
        single("included_stat", 1.0),
        single("extra_stat", 2.0),
    ], "English");
    assert_eq!(result.lines, vec!["1 included", "2 extra"]);
}

#[test]
fn unresolved_include_is_an_error() {
    let error = StatDescriptions::parse("include \"missing.txt\"\r\n").unwrap_err();
    assert!(matches!(error, QueryError::Internal(_)));
}
