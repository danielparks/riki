//! The parser that transforms the [`Cst`] into a [`Configuration`].

use super::lexer::{Diagnostic, TokenType};
use super::model::{
    Action, ConfigRule, Configuration, Identifier, MatcherStack, Parameters,
    ParseError, ParseResult, Setting, StringToken, Value,
};
use super::parser::{CNode, CNodeIter, Cst, NodeRef, Parser, Rule, RuleSide};
use std::fmt;

/// Parse configuration file contents and return rules or errors.
///
/// # Errors
///
/// Returns [`Diagnostic`]s that point out problems in `source`.
pub fn parse(source: &str) -> Result<Configuration<'_>, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let cst = Parser::parse(source, &mut diagnostics);
    if diagnostics.is_empty() {
        process_cst(&cst).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.into_diagnostic(source))
                .collect()
        })
    } else {
        Err(diagnostics)
    }
}

/// Process the CST from the parse into rules.
///
/// # Errors
///
/// Returns any errors encountered while processing the CST. See
/// [`super::model::SpannedError::into_diagnostic()`].
///
/// # Panics
///
/// Should only panic if [`Parser::parse()`] messed up.
pub fn process_cst<'src>(
    cst: &Cst<'src>,
) -> ParseResult<'src, Configuration<'src>> {
    let mut iter = cst.descendents(NodeRef::ROOT);
    let mut matcher_stack: MatcherStack = MatcherStack::empty();
    let mut rules: Configuration = Vec::new();
    let mut errors = Vec::new();
    while let Some(node) = iter.next() {
        use RuleSide::{Pop, Push};
        match node {
            CNode::Rule(Rule::Action, Push) => {
                match consume_matcher_rule(&mut iter, "").try_into() {
                    Ok(value) => {
                        rules.push(ConfigRule {
                            matcher: matcher_stack.clone(),
                            action: Action::Value(Value::Literal(value)),
                        });
                    }
                    Err(found) => errors.extend(found),
                }
                expect_rule(&mut iter, Rule::Action, Pop, " after action push");
            }
            CNode::Rule(Rule::Context, Push) => {
                match consume_matcher_rule(&mut iter, " in context rule")
                    .try_into()
                {
                    Ok(matcher) => matcher_stack.push(matcher),
                    Err(found) => errors.extend(found),
                }
                expect_token(&mut iter, TokenType::LBrace, " in context rule");
            }
            CNode::Rule(Rule::Context | Rule::Rule, Pop) => {
                assert!(
                    matcher_stack.pop().is_some(),
                    "matcher stack empty; errors: {errors:?}"
                );
            }
            CNode::Rule(
                rule @ (Rule::Block | Rule::Line | Rule::Params),
                _,
            ) => {
                panic!("{rule:?} rule should be elided by grammar")
            }
            CNode::Rule(Rule::Error, _) => {
                panic!("found error rule (should have prevented processing)")
            }
            CNode::Rule(Rule::File, _) => {
                // Either start or end of file
                assert!(
                    matcher_stack.is_empty(),
                    "matcher stack must be empty at start and end of file"
                );
            }
            CNode::Rule(Rule::Function, Push) => {
                match consume_function_contents(&mut iter) {
                    Ok(value) => {
                        rules.push(ConfigRule {
                            matcher: matcher_stack.clone(),
                            action: Action::Value(value),
                        });
                    }
                    Err(found) => errors.extend(found),
                }
            }
            CNode::Rule(Rule::Matcher, Push) => {
                panic!("unexpected matcher rule");
            }
            CNode::Rule(
                rule @ (Rule::Action
                | Rule::Function
                | Rule::Matcher
                | Rule::Set
                | Rule::Value),
                Pop,
            ) => panic!("{rule:?} pop rule should have already been consumed"),
            CNode::Rule(Rule::Rule, Push) => {
                match consume_matcher_rule(&mut iter, " in rule rule")
                    .try_into()
                {
                    Ok(matcher) => matcher_stack.push(matcher),
                    Err(found) => errors.extend(found),
                }
                // Value or function next — let this loop take care of it.
            }
            CNode::Rule(Rule::Set, Push) => {
                match consume_set_contents(&mut iter) {
                    Ok(setting) => {
                        rules.push(ConfigRule {
                            matcher: matcher_stack.clone(),
                            action: Action::Setting(setting),
                        });
                    }
                    Err(found) => errors.extend(found),
                }
            }
            CNode::Rule(Rule::Value, Push) => {
                match consume_value_contents(&mut iter) {
                    Ok(value) => {
                        rules.push(ConfigRule {
                            matcher: matcher_stack.clone(),
                            action: Action::Value(value),
                        });
                    }
                    Err(found) => errors.extend(found),
                }
            }
            CNode::Token(
                token @ (TokenType::Identifier
                | TokenType::Path
                | TokenType::BareGlob
                | TokenType::QuotedDouble
                | TokenType::QuotedSingle
                | TokenType::LBrace
                | TokenType::LParen
                | TokenType::RParen
                | TokenType::Comma
                | TokenType::Equal),
                _,
            ) => panic!("unexpected {token:?} token outside of a rule"),
            CNode::Token(token @ (TokenType::Error | TokenType::EOF), _) => {
                panic!("unexpected {token:?} token")
            }
            CNode::Token(TokenType::Newline | TokenType::RBrace, _) => {
                // Ignore
            }
        }
    }

    if errors.is_empty() {
        Ok(rules)
    } else {
        Err(errors)
    }
}

/// Consume contents of a set rule.
fn consume_set_contents<'src>(
    iter: &mut CNodeIter<'_, 'src>,
) -> ParseResult<'src, Setting<'src>> {
    let variable = consume_identifier(iter, " for set variable");
    expect_token(iter, TokenType::Equal, " in set rule");

    let result = match iter.next() {
        Some(CNode::Rule(Rule::Function, RuleSide::Push)) => {
            consume_function_contents(iter)
        }
        Some(CNode::Rule(Rule::Value, RuleSide::Push)) => {
            consume_value_contents(iter)
        }
        Some(other) => {
            panic!("expected function or value push rule; got {other:?}");
        }
        None => panic!("expected function or value push rule; got end of file"),
    };

    expect_rule(iter, Rule::Set, RuleSide::Pop, " after set push rule");

    result.and_then(|value| (variable, value).try_into())
}

/// Consume contents of a function rule.
fn consume_function_contents<'src>(
    iter: &mut CNodeIter<'_, 'src>,
) -> ParseResult<'src, Value<'src>> {
    // Get identifier (name of function)
    let identifier = consume_identifier(iter, " for function identifier");
    let mut parameters = Parameters::new();
    let mut errors = Vec::new();
    // Get '('
    expect_token(iter, TokenType::LParen, " in function rule");

    // Process all the nodes inside the parentheses.
    while let Some(node) = iter.next() {
        match node {
            CNode::Rule(Rule::Value, RuleSide::Push) => {
                match consume_value_contents(iter) {
                    Ok(value) => parameters.push(value),
                    Err(found) => errors.extend(found),
                }
            }
            CNode::Rule(Rule::Value, RuleSide::Pop) => {
                panic!("unexpected value rule pop; should have been consumed")
            }
            CNode::Rule(Rule::Function, RuleSide::Push) => {
                match consume_function_contents(iter) {
                    Ok(value) => parameters.push(value),
                    Err(found) => errors.extend(found),
                }
            }
            // Ignore tokens — we rely on the parser grammar to make sure these
            // are in the correct places.
            CNode::Token(TokenType::Comma | TokenType::RParen, _) => {}
            // Find the end of the function. We always consume both in a pair
            // so we can never get one that corresponds to a different function.
            CNode::Rule(Rule::Function, RuleSide::Pop) => {
                return if errors.is_empty() {
                    Ok(Value::Function(
                        (identifier.clone(), parameters).try_into().map_err(
                            // FIXME wrong span! Generate a new span from
                            // identifier to the last RParen
                            |error: ParseError| error.spanned_s(identifier.0),
                        )?,
                    ))
                } else {
                    Err(errors)
                };
            }
            other => panic!("expected value rule, ',', or ')', got {other:?}"),
        }
    }
    panic!("expected value rule, ',', or ')', but the file ended")
}

/// Consume contents of a value rule.
fn consume_value_contents<'src>(
    iter: &mut CNodeIter<'_, 'src>,
) -> ParseResult<'src, Value<'src>> {
    let token = consume_string_token(iter, " in value");
    expect_rule(iter, Rule::Value, RuleSide::Pop, " after value token");
    Ok(Value::Literal(token.try_into()?))
}

/// Check that the next node is a matcher rule, then get the token it contains.
///
/// This doesn’t return `Matcher` because in one case it’s used to consume an
/// action value that the parser thinks is a matcher.
fn consume_matcher_rule<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> StringToken<'src> {
    expect_rule(iter, Rule::Matcher, RuleSide::Push, &context);
    let token =
        consume_string_token(iter, format!(" in matcher rule{context}"));
    expect_rule(iter, Rule::Matcher, RuleSide::Pop, " after matcher token");
    token
}

/// Consume an identifier token (e.g. a function name).
fn consume_identifier<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Identifier<'src> {
    consume_string_token(iter, &context)
        .try_into()
        .unwrap_or_else(|_| panic!("expected identifier token{context}"))
}

/// Check that the next node is a token and return it.
fn consume_string_token<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> StringToken<'src> {
    let next = iter.next();
    let Some(CNode::Token(token, src)) = next else {
        panic!("expected string token{context}, got {next:?}");
    };

    StringToken { string_type: token.try_into().unwrap(), src }
}

/// Check that the next node is a certain rule.
fn expect_rule<D: fmt::Display>(
    iter: &mut CNodeIter<'_, '_>,
    expected: Rule,
    expected_side: RuleSide,
    context: D,
) {
    let next = iter.next();
    let Some(CNode::Rule(rule, side)) = next else {
        panic!(
            "expected {expected:?} {expected_side:?} rule{context}, got {next:?}"
        );
    };
    assert_eq!(
        (expected, expected_side),
        (rule, side),
        "expected {expected:?} {expected_side:?} rule{context}, got {rule:?} {side:?} rule)"
    );
}

/// Check that the next node is a specific token and return it.
fn expect_token<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    expected: TokenType,
    context: D,
) -> &'src str {
    let next = iter.next();
    let Some(CNode::Token(token, src)) = next else {
        panic!("expected token{context}, got {next:?}");
    };
    assert_eq!(
        expected, token,
        "expected {expected:?} token{context}, got {token:?}"
    );
    src
}
