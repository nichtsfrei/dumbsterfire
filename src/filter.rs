use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("Unexpected character at position {pos}: {ch}")]
    UnexpectedChar { pos: usize, ch: char },

    #[error("Unexpected end of input at position {pos}")]
    UnexpectedEOF { pos: usize },

    #[error("Unknown operator '{op}' at position {pos}")]
    UnknownOperator { op: String, pos: usize },

    #[error("Expected symbol at position {pos}")]
    ExpectedSymbol { pos: usize },

    #[error("Expected string at position {pos}")]
    ExpectedString { pos: usize },
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum Operator {
    Contains,
    Is,
}

#[derive(Debug)]
pub struct Rule<'a> {
    fields: Vec<&'a str>,
    op: Operator,
    values: Vec<&'a str>,
}

#[derive(Debug)]
pub enum Filter<'a> {
    Rule(Rule<'a>),
    And(Vec<Filter<'a>>),
    Or(Vec<Filter<'a>>),
    Not(Box<Filter<'a>>),
}

pub enum CompareResult {
    Match,
    NoMatch,
    NotApplicable,
}

impl From<Option<bool>> for CompareResult {
    fn from(value: Option<bool>) -> Self {
        value
            .map(CompareResult::from)
            .unwrap_or(Self::NotApplicable)
    }
}

impl From<bool> for CompareResult {
    fn from(value: bool) -> Self {
        if value { Self::Match } else { Self::NoMatch }
    }
}
pub trait FieldComparer {
    fn compare_field<'a>(&'a self, op: &Operator, field: &'a str, value: &'a str) -> CompareResult;
}

impl Filter<'_> {
    pub fn eval<R>(&self, retriever: &R) -> bool
    where
        R: FieldComparer,
    {
        match self {
            Filter::Rule(rule) => rule.fields.iter().any(|field| {
                rule.values
                    .iter()
                    .filter_map(|x| match retriever.compare_field(&rule.op, field, x) {
                        CompareResult::Match => Some(true),
                        CompareResult::NoMatch => Some(false),
                        CompareResult::NotApplicable => None,
                    })
                    .any(|x| x)
            }),
            Filter::And(filters) => {
                for f in filters {
                    if !f.eval(retriever) {
                        return false;
                    }
                }
                true
            }
            Filter::Or(filters) => filters.iter().any(|x| x.eval(retriever)),
            Filter::Not(filter) => !filter.eval(retriever),
        }
    }
}

pub struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_ignorable(&mut self) {
        loop {
            // Skip normal whitespace first
            while let Some(ch) = self.peek_char() {
                if !ch.is_whitespace() {
                    break;
                }
                self.next_char();
            }

            // If comment starts, consume until newline
            if self.peek_char() == Some(';') {
                while let Some(ch) = self.next_char() {
                    if ch == '\n' {
                        break;
                    }
                }
                // Continue loop in case more whitespace/comments follow
                continue;
            }

            break;
        }
    }

    fn parse_symbol(&mut self) -> Result<&'a str, FilterError> {
        self.skip_ignorable();
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || ch == '(' || ch == ')' {
                break;
            }
            self.next_char();
        }
        if start == self.pos {
            Err(FilterError::ExpectedSymbol { pos: self.pos })
        } else {
            Ok(&self.input[start..self.pos])
        }
    }

    fn parse_string(&mut self) -> Result<&'a str, FilterError> {
        self.skip_ignorable();
        if self.next_char() != Some('"') {
            return Err(FilterError::ExpectedString { pos: self.pos });
        }
        let start = self.pos;
        while let Some(ch) = self.next_char() {
            if ch == '"' {
                let end = self.pos - 1;
                return Ok(&self.input[start..end]);
            }
        }
        Err(FilterError::UnexpectedEOF { pos: self.pos })
    }

    pub fn parse(&mut self) -> Result<Filter<'a>, FilterError> {
        self.skip_ignorable();
        if self.next_char() != Some('(') {
            return Err(FilterError::UnexpectedChar {
                pos: self.pos,
                ch: '(',
            });
        }
        self.parse_list()
    }

    fn check_consum_end(&mut self) -> Result<(), FilterError> {
        let result = self
            .next_char()
            .ok_or(FilterError::UnexpectedEOF { pos: self.pos })?;
        if result != ')' {
            Err(FilterError::UnexpectedChar {
                pos: self.pos,
                ch: result,
            })
        } else {
            Ok(())
        }
    }

    fn parse_children<F>(&mut self, f: F) -> Result<Filter<'a>, FilterError>
    where
        F: FnOnce(Vec<Filter<'a>>) -> Filter<'a>,
    {
        let mut children = Vec::new();
        loop {
            self.skip_ignorable();
            if self.peek_char() == Some(')') {
                break;
            }
            children.push(self.parse()?);
        }
        self.check_consum_end()?;
        Ok(f(children))
    }

    fn parse_fields(&mut self) -> Result<Vec<&'a str>, FilterError> {
        self.skip_ignorable();
        Ok(if self.peek_char() == Some('(') {
            self.next_char();
            let mut flds = Vec::new();
            while self.peek_char() != Some(')') {
                flds.push(self.parse_symbol()?);
                self.skip_ignorable();
            }

            self.check_consum_end()?;
            flds
        } else {
            vec![self.parse_symbol()?]
        })
    }

    fn parse_string_values(&mut self) -> Result<Vec<&'a str>, FilterError> {
        let mut values = Vec::new();
        while self.peek_char() != Some(')') {
            values.push(self.parse_string()?);
            self.skip_ignorable();
        }
        Ok(values)
    }

    fn parse_rule(&mut self, op: Operator) -> Result<Filter<'a>, FilterError> {
        let fields = self.parse_fields()?;
        let values = self.parse_string_values()?;
        self.check_consum_end()?;

        Ok(Filter::Rule(Rule { fields, op, values }))
    }

    fn parse_list(&mut self) -> Result<Filter<'a>, FilterError> {
        let op = self.parse_symbol()?;
        match op {
            "and" => self.parse_children(Filter::And),
            "or" => self.parse_children(Filter::Or),

            "not" => {
                let child = self.parse()?;
                self.check_consum_end()?;
                Ok(Filter::Not(Box::new(child)))
            }
            "contains" => self.parse_rule(Operator::Contains),
            "is" => self.parse_rule(Operator::Is),

            _ => Err(FilterError::UnknownOperator {
                op: op.to_string(),
                pos: self.pos,
            }),
        }
    }
}

pub fn parse<'a, T>(value: &'a T) -> Result<Filter<'a>, FilterError>
where
    T: AsRef<str>,
{
    let mut parser = Parser::new(value.as_ref());
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    pub struct DummyComparer {
        pub data: HashMap<&'static str, &'static str>,
    }

    impl FieldComparer for DummyComparer {
        fn compare_field<'a>(
            &'a self,
            op: &Operator,
            field: &'a str,
            value: &'a str,
        ) -> CompareResult {
            match self.data.get(field) {
                Some(field_value) => match op {
                    Operator::Contains => field_value.contains(value).into(),
                    Operator::Is => (field_value == &value).into(),
                },
                None => CompareResult::NotApplicable,
            }
        }
    }
}

#[cfg(test)]
mod parse {

    use super::{Filter, FilterError, Parser};

    #[test]
    fn and_or_not() -> Result<(), FilterError> {
        let input = r#"(and
        (or
            (contains (subject) "foo")
            (contains (subject body) "bar")
        )
        (not (contains subject "fnord"))
        (is (from) "v3")
        )"#;

        let mut parser = Parser::new(input);
        let ast = parser.parse()?;
        dbg!(&ast);

        match ast {
            Filter::And(children) => {
                assert_eq!(children.len(), 3);
                assert!(matches!(children[0], Filter::Or(_)));
                assert!(matches!(children[1], Filter::Not(_)));
                assert!(matches!(children[2], Filter::Rule(_)));
            }
            _ => panic!("Expected top-level AND filter"),
        }

        Ok(())
    }
}

#[cfg(test)]
mod eval {
    use std::collections::HashMap;

    use crate::filter::parse;

    use super::tests::DummyComparer;

    #[test]
    fn contains() {
        let filter = parse(&r#"(contains (subject) "hello")"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([("subject", "hello world")]),
        };

        assert!(filter.eval(&comparer));
    }

    #[test]
    fn contains_no_match() {
        let filter = parse(&r#"(contains (subject) "bye")"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([("subject", "hello world")]),
        };

        assert!(!filter.eval(&comparer));
    }

    #[test]
    fn and() {
        let filter = parse(&r#"(and (contains (subject) "hello") (is (from) "alice"))"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([("subject", "hello world"), ("from", "alice")]),
        };

        assert!(filter.eval(&comparer));
    }

    #[test]
    fn or() {
        let filter = parse(&r#"(or (contains (subject) "nope") (is (from) "alice"))"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([("from", "alice")]),
        };

        assert!(filter.eval(&comparer));
    }

    #[test]
    fn not() {
        let filter = parse(&r#"(not (contains (subject) "spam"))"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([("subject", "hello world")]),
        };

        assert!(filter.eval(&comparer));
    }

    #[test]
    fn not_applicable() {
        let filter = parse(&r#"(contains (missing) "foo")"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::new(),
        };

        assert!(!filter.eval(&comparer));
    }

    #[test]
    fn multi_field_multi_value() {
        let filter = parse(&r#"(contains (subject body) "foo" "bar")"#).unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([("subject", "xxx"), ("body", "bar baz")]),
        };

        assert!(filter.eval(&comparer));
    }

    #[test]
    fn with_comments() {
        let filter = parse(
            &r#"
(and
  ;; Check for indicator in subject, from, ... those are in the subject or content
  (contains (subject content) "invoice" "rechnung" "billing")
  ;; be picky ... only when they have pdf or at least mention an IBAN or paypal
  (or 
    (contains (attachment_names) "pdf")
    (contains content "paypal" "IBAN")
  )
)"#,
        )
        .unwrap();

        let comparer = DummyComparer {
            data: HashMap::from([
                ("subject", "your invoice is ready"),
                ("content", "IBAN"),
                ("from", "telekom"),
            ]),
        };

        assert!(filter.eval(&comparer));
    }
}
