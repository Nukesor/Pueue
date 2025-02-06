use crate::prelude::*;

use pest::iterators::Pair;

use super::{QueryResult, Rule};

/// An enum indicating whether the first or the first or the last tasks in the
/// `pueue status` command should be shown.
pub enum Limit {
    First,
    Last,
}

/// Parse a limit condition.
///
/// This limit syntax looks like this:
/// `[first|last] [count]`
///
/// The data structure looks something like this:
/// Pair {
///     rule: limit_condition,
///     span: Span {
///         str: "first 2",
///         start: 0,
///         end: 7,
///     },
///     inner: [
///         Pair {
///             rule: first,
///             span: Span {
///                 str: "first",
///                 start: 0,
///                 end: 5,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: limit_count,
///             span: Span {
///                 str: "2",
///                 start: 6,
///                 end: 7,
///             },
///             inner: [],
///         },
///     ],
/// }
pub fn limit(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    let mut limit_condition = section.into_inner();
    // The first word should be the `label` keyword.
    let direction = limit_condition.next().unwrap();
    let direction = match direction.as_rule() {
        Rule::first => Limit::First,
        Rule::last => Limit::Last,
        _ => bail!("Expected either of [first|last]"),
    };

    // Get the label we should order by.
    let amount = limit_condition.next().unwrap();
    let count: usize = amount
        .as_str()
        .parse()
        .context("Expected a number 0 for limit condition")?;

    if count == 0 {
        bail!("Expected a number >0 for limit condition");
    }

    query_result.limit = Some((direction, count));
    Ok(())
}
