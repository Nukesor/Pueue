use crate::prelude::*;

use pest::iterators::Pair;

use super::{QueryResult, Rule};

pub fn apply(section: Pair<'_, Rule>, query_result: &mut QueryResult) -> Result<()> {
    // This query is expected to be structured like this:
    // `columns = [(column (, column)*]`
    let mut columns_pairs = section.into_inner();
    // Pop the `column` and `=`
    let _columns_word = columns_pairs.next().unwrap();
    let _equals = columns_pairs.next().unwrap();

    // Get the list of columns.
    let multiple_columns = columns_pairs.next().unwrap();
    // Extract all columns from the multiple_columns.inner iterator
    // The structure is like this
    // ```
    // Pair {
    //  rule: multiple_columns,
    //  span: Span {
    //      str: "id,status",
    //      start: 7,
    //      end: 16,
    //  },
    //  inner: [
    //      Pair {
    //          rule: column,
    //          span: Span {
    //              str: "id",
    //              start: 7,
    //              end: 9,
    //          },
    //          inner: [
    //              Pair {
    //                  rule: id,
    //                  span: Span {
    //                      str: "id",
    //                      start: 7,
    //                      end: 9,
    //                  },
    //                  inner: [],
    //              },
    //          ],
    //      },
    //      ...
    //      ]
    // }
    // ```
    let mut columns = multiple_columns
        .into_inner()
        .map(|pair| pair.into_inner().next().unwrap().as_rule())
        .collect::<Vec<Rule>>();

    query_result.selected_columns.append(&mut columns);

    Ok(())
}
