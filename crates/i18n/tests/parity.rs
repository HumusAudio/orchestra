// SPDX-License-Identifier: GPL-3.0-or-later

//! Translation parity tests (ADR-0008, CLAUDE.md §7).
//!
//! These tests fail when:
//! 1. a message or attribute exists in one catalog but not in another (in either direction:
//!    a key missing from a translation, or a stale key left in a translation after it was
//!    removed from en-US);
//! 2. the set of **variables** (`$count`, …) used by a message differs between en-US and a
//!    translation — a translation that drops `$count` would silently show a wrong number, one
//!    that invents `$total` would fail at run time;
//! 3. a message cannot be formatted in some language with the arguments en-US needs.
//!
//! We parse the catalogs with `fluent-syntax` (the same parser `fluent-bundle` uses) and walk the
//! AST, instead of using regular expressions, so that selectors, nested placeables and attributes
//! are handled exactly as Fluent understands them.

// This whole file is test code, but clippy only recognises `#[test]` functions as such: the
// helper `collect` legitimately panics (with a readable message) on a broken catalog, so the
// workspace's library rule against `panic!` is lifted for this file only.
#![allow(
    clippy::panic,
    reason = "test helper: panicking with a readable message is how a broken catalog is reported"
)]

use std::collections::{BTreeMap, BTreeSet};

use fluent_syntax::ast;
use orchestra_i18n::{FluentArgs, Locale, Localizer};

/// For each message key (`id` or `id.attribute`), the set of variable names it references.
///
/// `BTreeMap`/`BTreeSet` (sorted) rather than hash maps so that failure messages list keys in a
/// stable, readable order.
type KeyVariables = BTreeMap<String, BTreeSet<String>>;

/// Parses every catalog file of `locale` and collects its keys and their variables.
///
/// Terms (`-brand-name`) are included with their leading dash, because a term defined in one
/// language must exist in all of them too.
fn collect(locale: Locale) -> KeyVariables {
    let mut keys = KeyVariables::new();
    for source in locale.catalog_sources() {
        let resource = fluent_syntax::parser::parse(*source).unwrap_or_else(|(_, errors)| {
            panic!("{locale}: Fluent syntax errors: {errors:?}");
        });
        for entry in resource.body {
            let (id, value, attributes) = match entry {
                ast::Entry::Message(m) => (m.id.name.to_owned(), m.value, m.attributes),
                ast::Entry::Term(t) => (format!("-{}", t.id.name), Some(t.value), t.attributes),
                // Comments and junk carry no keys (junk would already have been a parse error).
                _ => continue,
            };
            if let Some(pattern) = value {
                let mut vars = BTreeSet::new();
                pattern_variables(&pattern, &mut vars);
                assert!(
                    keys.insert(id.clone(), vars).is_none(),
                    "{locale}: key `{id}` defined twice"
                );
            }
            for attribute in attributes {
                let mut vars = BTreeSet::new();
                pattern_variables(&attribute.value, &mut vars);
                keys.insert(format!("{id}.{}", attribute.id.name), vars);
            }
        }
    }
    keys
}

/// Adds to `out` every variable referenced anywhere inside `pattern` (all selector variants).
fn pattern_variables(pattern: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_variables(expression, out);
        }
    }
}

/// Adds to `out` the variables of an expression: the selector and every variant of a select
/// expression, or the inline expression itself.
fn expression_variables(expression: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
    match expression {
        ast::Expression::Select { selector, variants } => {
            inline_variables(selector, out);
            for variant in variants {
                pattern_variables(&variant.value, out);
            }
        }
        ast::Expression::Inline(inline) => inline_variables(inline, out),
    }
}

/// Adds to `out` the variables of an inline expression, recursing into function arguments,
/// term arguments and nested placeables.
fn inline_variables(inline: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_owned());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            call_variables(arguments, out);
        }
        ast::InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => call_variables(arguments, out),
        ast::InlineExpression::Placeable { expression } => expression_variables(expression, out),
        // Literals and message/term references without arguments contain no variables.
        _ => {}
    }
}

/// Adds to `out` the variables used as positional or named call arguments.
fn call_variables(arguments: &ast::CallArguments<&str>, out: &mut BTreeSet<String>) {
    for positional in &arguments.positional {
        inline_variables(positional, out);
    }
    for named in &arguments.named {
        inline_variables(&named.value, out);
    }
}

/// Every key of en-US exists in every other catalog, and no catalog has extra keys.
#[test]
fn every_key_exists_in_every_language() {
    let source = collect(Locale::EnUs);
    assert!(!source.is_empty(), "the en-US catalog is empty");
    let mut problems = Vec::new();
    for locale in Locale::ALL.into_iter().skip(1) {
        let translated = collect(locale);
        for key in source.keys().filter(|k| !translated.contains_key(*k)) {
            problems.push(format!("{locale}: missing key `{key}`"));
        }
        for key in translated.keys().filter(|k| !source.contains_key(*k)) {
            problems.push(format!("{locale}: key `{key}` does not exist in en-US"));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Every message uses exactly the same variables in every language.
#[test]
fn variables_match_the_source_language() {
    let source = collect(Locale::EnUs);
    let mut problems = Vec::new();
    for locale in Locale::ALL.into_iter().skip(1) {
        let translated = collect(locale);
        for (key, vars) in &source {
            if let Some(other) = translated.get(key)
                && other != vars
            {
                problems.push(format!(
                    "{locale}: `{key}` uses variables {other:?}, en-US uses {vars:?}"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Every message formats without errors in every language, given the arguments that en-US
/// uses. Each variable receives a sample number, which exercises plural selectors; when
/// messages with string variables appear, this helper will need per-variable sample values.
#[test]
fn every_message_formats_in_every_language() {
    let source = collect(Locale::EnUs);
    let mut problems = Vec::new();
    for locale in Locale::ALL {
        let localizer = Localizer::new(locale).unwrap();
        // Attributes and terms are not formatted directly through `format` (it formats message
        // values only), so only plain message keys are exercised here.
        for (key, vars) in source
            .iter()
            .filter(|(k, _)| !k.contains('.') && !k.starts_with('-'))
        {
            // Try a few counts so that every plural category (one/other, and zero via French)
            // is reached at least once.
            for sample in [0, 1, 2, 5, 21] {
                let mut args = FluentArgs::new();
                for var in vars {
                    args.set(var.as_str(), sample);
                }
                let args = (!vars.is_empty()).then_some(&args);
                if let Err(e) = localizer.format(key, args) {
                    problems.push(format!("{locale}: {e}"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
