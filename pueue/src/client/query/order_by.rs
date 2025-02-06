#![allow(bindings_with_variant_name)]
use crate::prelude::*;

use pest::iterators::Pair;

use super::{QueryResult, Rule};

pub enum Direction {
    Ascending,
    Descending,
}

/// Parse an order_by condition.
///
/// This filter syntax looks like this:
/// `order_by [column] [asc|desc]`
///
/// The data structure looks something like this:
/// Pair {
///     rule: order_by_condition,
///     span: Span {
///         str: "order_by label desc",
///         start: 0,
///         end: 19,
///     },
///     inner: [
///         Pair {
///             rule: order_by,
///             span: Span {
///                 str: "order_by",
///                 start: 0,
///                 end: 8,
///             },
///             inner: [],
///         },
///         Pair {
///             rule: column,
///             span: Span {
///                 str: "label",
///                 start: 9,
///                 end: 14,
///             },
///             inner: [
///                 Pair {
///                     rule: column_label,
///                     span: Span {
///                         str: "label",
///                         start: 9,
///                         end: 14,
///                     },
///                     inner: [],
///                 },
///             ],
///         },
///         Pair {
///             rule: descending,
///             span: Span {
///                 str: "desc",
///                 start: 15,
///                 end: 19,
///             },
///             inner: [],
///         },
///     ],
/// }
pub fn order_by(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    let mut order_by_condition = section.into_inner();
    // The first word should be the `order_by` keyword.
    let _order_by = order_by_condition.next().unwrap();

    // Get the column we should order by.
    // The column is wrapped by a `Rule::column` keyword.
    let column_keyword = order_by_condition.next().unwrap();
    let column = column_keyword.into_inner().next().unwrap().as_rule();

    // Get the direction we should order by.
    // If no direction is provided, default to `Ascending`.
    let direction = match order_by_condition.next().map(|pair| pair.as_rule()) {
        Some(Rule::ascending) => Direction::Ascending,
        Some(Rule::descending) => Direction::Descending,
        _ => Direction::Ascending,
    };

    query_result.order_by = Some((column, direction));
    Ok(())
}
