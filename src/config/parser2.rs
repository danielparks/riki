//! The parser that transforms the [`Cst`] into a [`Configuration`].

use super::lexer::{Diagnostic, TokenType};
use super::model::{
    Action, ConfigRule, Configuration, Identifier, Matcher, MatcherStack,
    Parameters, Setting, Value, Word,
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
        Ok(process_cst(&cst))
    } else {
        Err(diagnostics)
    }
}

/// Process the CST from the parse into rules.
///
/// # Panics
///
/// Should only panic if [`Parser::parse()`] messed up.
#[must_use]
pub fn process_cst<'src>(cst: &Cst<'src>) -> Configuration<'src> {
    let mut iter = cst.descendents(NodeRef::ROOT);
    let mut matcher_stack: MatcherStack = MatcherStack::empty();
    let mut rules: Configuration = Vec::new();
    while let Some(node) = iter.next() {
        use RuleSide::{Pop, Push};
        #[expect(clippy::match_same_arms, reason = "clarity")]
        match node {
            CNode::Rule(Rule::Action, Push) => {
                let action =
                    Action::Value(consume_matcher_rule(&mut iter, "").into());
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
                expect_rule(&mut iter, Rule::Action, Pop, " after action push");
            }
            CNode::Rule(Rule::Context, Push) => {
                matcher_stack.push(Matcher(consume_matcher_rule(
                    &mut iter,
                    " in context rule",
                )));
                expect_token(&mut iter, TokenType::LBrace, " in context rule");
            }
            CNode::Rule(Rule::Context, Pop) => {
                assert!(matcher_stack.pop().is_some());
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
                let action =
                    Action::Value(consume_function_contents(&mut iter));
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
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
                matcher_stack.push(Matcher(consume_matcher_rule(
                    &mut iter,
                    " in rule rule",
                )));
                // Value or function next — let this loop take care of it.
            }
            CNode::Rule(Rule::Rule, Pop) => {
                assert!(matcher_stack.pop().is_some());
            }
            CNode::Rule(Rule::Set, Push) => {
                let action = Action::Setting(consume_set_contents(&mut iter));
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
            }
            CNode::Rule(Rule::Value, Push) => {
                let action = Action::Value(consume_value_contents(&mut iter));
                rules.push(ConfigRule {
                    matcher: matcher_stack.clone(),
                    action,
                });
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

    rules
}

/// Consume contents of a set rule.
fn consume_set_contents<'src>(iter: &mut CNodeIter<'_, 'src>) -> Setting<'src> {
    let variable = consume_identifier(iter, " for set variable");
    expect_token(iter, TokenType::Equal, " in set rule");

    let value = match iter.next() {
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
    Setting { variable, value }
}

/// Consume contents of a function rule.
fn consume_function_contents<'src>(
    iter: &mut CNodeIter<'_, 'src>,
) -> Value<'src> {
    // Get identifier (name of function)
    let identifier = consume_identifier(iter, " for function identifier");
    let mut parameters = Parameters::new();
    // Get '('
    expect_token(iter, TokenType::LParen, " in function rule");

    // Process all the nodes inside the parentheses.
    while let Some(node) = iter.next() {
        match node {
            CNode::Rule(Rule::Value, RuleSide::Push) => {
                parameters.push(consume_value_contents(iter));
            }
            CNode::Rule(Rule::Value, RuleSide::Pop) => {
                panic!("unexpected value rule pop; should have been consumed")
            }
            CNode::Rule(Rule::Function, RuleSide::Push) => {
                parameters.push(consume_function_contents(iter));
            }
            // Ignore tokens — we rely on the parser grammar to make sure these
            // are in the correct places.
            CNode::Token(TokenType::Comma | TokenType::RParen, _) => {}
            // Find the end of the function. We always consume both in a pair
            // so we can never get one that corresponds to a different function.
            CNode::Rule(Rule::Function, RuleSide::Pop) => {
                return Value::Function(identifier, parameters);
            }
            other => panic!("expected value rule, ',', or ')', got {other:?}"),
        }
    }
    panic!("expected value rule, ',', or ')', but the file ended")
}

/// Consume contents of a value rule.
fn consume_value_contents<'src>(iter: &mut CNodeIter<'_, 'src>) -> Value<'src> {
    let word = consume_word_token(iter, " in value");
    expect_rule(iter, Rule::Value, RuleSide::Pop, " after value token");
    word.into()
}

/// Check that the next node is a matcher rule, then get the token it contains.
///
/// This doesn’t return `Matcher` because in one case it’s used to consume an
/// action value that the parser thinks is a matcher.
fn consume_matcher_rule<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Word<'src> {
    expect_rule(iter, Rule::Matcher, RuleSide::Push, &context);
    let word = consume_word_token(iter, format!(" in matcher rule{context}"));
    expect_rule(iter, Rule::Matcher, RuleSide::Pop, " after matcher token");
    word
}

/// Consume a bare word token.
fn consume_identifier<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Identifier<'src> {
    consume_word_token(iter, &context)
        .try_into()
        .unwrap_or_else(|_| panic!("expected identifier token{context}"))
}

/// Check that the next node is a token and return it.
fn consume_word_token<'src, D: fmt::Display>(
    iter: &mut CNodeIter<'_, 'src>,
    context: D,
) -> Word<'src> {
    let next = iter.next();
    let Some(CNode::Token(token, src)) = next else {
        panic!("expected word token{context}, got {next:?}");
    };

    Word { type_: token.try_into().unwrap(), src }
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
