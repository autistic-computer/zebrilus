use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

use super::catalog::{Component, NAMES, Param, signature};
use super::document::{Document, Message, SpecError};

const SURFACE: &str = "main";
const ROOT: &str = "root";
const MAX_ALIASES: usize = 32;
const MARKS: &str = "[]{}(),:=";
const PATH_MARKS: &str = "_/-.~%";

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text(String),
    Number(Value),
    Path(String),
    Word(String),
    Mark(char),
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Literal(Value),
    Path(String),
    List(Vec<Expr>),
    Map(Vec<(String, Expr)>),
    Call(String, Vec<Argument>),
    Name(String),
}

type Argument = (Option<String>, Expr);

enum Statement {
    Assign(String, Expr),
    Data(String, Expr),
    Bare,
}

fn at_line(line: usize, message: impl AsRef<str>) -> SpecError {
    SpecError::new(format!("line {line}: {}", message.as_ref()))
}

fn tokens(text: &str) -> Result<Vec<(Token, usize)>, SpecError> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut line = 1;
    while let Some(&first) = chars.get(at) {
        let second = chars.get(at + 1).copied();
        if first == '\n' {
            line += 1;
            at += 1;
        } else if first.is_whitespace() {
            at += 1;
        } else if first == '#' || (first == '/' && second == Some('/')) {
            while chars.get(at).is_some_and(|next| *next != '\n') {
                at += 1;
            }
        } else if first == '"' || (first == 'r' && second == Some('"')) {
            let raw = first == 'r';
            let (text, end, lines) = string(&chars, at + usize::from(raw), raw, line)?;
            tokens.push((Token::Text(text), line));
            line += lines;
            at = end;
        } else if first.is_ascii_digit()
            || (first == '-' && second.is_some_and(|next| next.is_ascii_digit()))
        {
            let start = at;
            at += 1;
            while chars
                .get(at)
                .is_some_and(|next| next.is_ascii_digit() || *next == '.')
            {
                at += 1;
            }
            let literal: String = chars[start..at].iter().collect();
            let number = literal
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| literal.parse::<f64>().map(Value::from))
                .map_err(|_| at_line(line, format!("`{literal}` is not a number")))?;
            tokens.push((Token::Number(number), line));
        } else if first == '$' {
            let start = at + 1;
            at = start;
            while chars
                .get(at)
                .is_some_and(|next| next.is_alphanumeric() || PATH_MARKS.contains(*next))
            {
                at += 1;
            }
            tokens.push((Token::Path(chars[start..at].iter().collect()), line));
        } else if first.is_alphabetic() || first == '_' || first == '@' {
            let start = at;
            at += 1;
            while chars
                .get(at)
                .is_some_and(|next| next.is_alphanumeric() || *next == '_')
            {
                at += 1;
            }
            tokens.push((Token::Word(chars[start..at].iter().collect()), line));
        } else if MARKS.contains(first) {
            tokens.push((Token::Mark(first), line));
            at += 1;
        } else {
            return Err(at_line(line, format!("cannot read `{first}`")));
        }
    }
    Ok(tokens)
}

fn string(
    chars: &[char],
    start: usize,
    raw: bool,
    line: usize,
) -> Result<(String, usize, usize), SpecError> {
    let triple = chars[start..].starts_with(&['"', '"', '"']);
    let quote = if triple { 3 } else { 1 };
    let mut at = start + quote;
    let mut text = String::new();
    let mut lines = 0;
    loop {
        let unclosed = || at_line(line, "a string is never closed");
        let next = *chars.get(at).ok_or_else(unclosed)?;
        if next == '"' && (!triple || chars[at..].starts_with(&['"', '"', '"'])) {
            return Ok((text, at + quote, lines));
        }
        if next == '\n' {
            if !triple {
                return Err(unclosed());
            }
            lines += 1;
        }
        if next == '\\' && !raw {
            text.push(match *chars.get(at + 1).ok_or_else(unclosed)? {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            at += 2;
        } else {
            text.push(next);
            at += 1;
        }
    }
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    at: usize,
}

impl Parser {
    fn peek(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.at + offset).map(|(token, _)| token)
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.at)
            .or_else(|| self.tokens.last())
            .map_or(1, |(_, line)| *line)
    }

    fn mark(&self, offset: usize, mark: char) -> bool {
        self.peek(offset) == Some(&Token::Mark(mark))
    }

    fn program(&mut self) -> Result<Vec<Statement>, SpecError> {
        let mut statements = Vec::new();
        while let Some(token) = self.peek(0) {
            let target = match token {
                Token::Word(name) if self.mark(1, '=') => Some((name.clone(), false)),
                Token::Path(path) if self.mark(1, '=') => Some((path.clone(), true)),
                _ => None,
            };
            statements.push(if let Some((name, data)) = target {
                self.at += 2;
                let value = self.expression()?;
                if data {
                    Statement::Data(name, value)
                } else {
                    Statement::Assign(name, value)
                }
            } else {
                self.expression()?;
                Statement::Bare
            });
        }
        Ok(statements)
    }

    fn sequence<T>(
        &mut self,
        close: char,
        item: fn(&mut Self) -> Result<T, SpecError>,
    ) -> Result<Vec<T>, SpecError> {
        let mut items = Vec::new();
        loop {
            if self.mark(0, close) {
                self.at += 1;
                return Ok(items);
            }
            if self.peek(0).is_none() {
                return Err(at_line(self.line(), format!("`{close}` is missing")));
            }
            items.push(item(self)?);
            if self.mark(0, ',') {
                self.at += 1;
            }
        }
    }

    fn entry(&mut self) -> Result<(String, Expr), SpecError> {
        let line = self.line();
        let key = match self.peek(0) {
            Some(Token::Word(key) | Token::Text(key)) => key.clone(),
            _ => return Err(at_line(line, "a map key must be a name or a string")),
        };
        if !self.mark(1, ':') {
            return Err(at_line(line, format!("`{key}` needs a `:` and a value")));
        }
        self.at += 2;
        Ok((key, self.expression()?))
    }

    fn argument(&mut self) -> Result<Argument, SpecError> {
        if let Some(Token::Word(name)) = self.peek(0)
            && self.mark(1, '=')
        {
            let name = name.clone();
            self.at += 2;
            return Ok((Some(name), self.expression()?));
        }
        Ok((None, self.expression()?))
    }

    fn expression(&mut self) -> Result<Expr, SpecError> {
        let line = self.line();
        let token = self.peek(0).cloned();
        self.at += 1;
        match token {
            Some(Token::Mark('[')) => Ok(Expr::List(self.sequence(']', Self::expression)?)),
            Some(Token::Mark('{')) => Ok(Expr::Map(self.sequence('}', Self::entry)?)),
            Some(Token::Text(text)) => Ok(Expr::Literal(Value::String(text))),
            Some(Token::Number(number)) => Ok(Expr::Literal(number)),
            Some(Token::Path(path)) => Ok(Expr::Path(path)),
            Some(Token::Word(word)) if self.mark(0, '(') => {
                self.at += 1;
                Ok(Expr::Call(word, self.sequence(')', Self::argument)?))
            }
            Some(Token::Word(word)) => Ok(match word.as_str() {
                "true" => Expr::Literal(Value::Bool(true)),
                "false" => Expr::Literal(Value::Bool(false)),
                "null" => Expr::Literal(Value::Null),
                _ => Expr::Name(word),
            }),
            Some(Token::Mark(mark)) => Err(at_line(line, format!("unexpected `{mark}`"))),
            None => Err(at_line(line, "the block ends early")),
        }
    }
}

#[derive(Default)]
struct Output {
    components: Vec<Value>,
    built: HashSet<String>,
    inline: usize,
}

#[derive(Default)]
struct Program {
    variables: HashMap<String, Expr>,
}

impl Program {
    fn resolve<'program>(
        &'program self,
        mut expr: &'program Expr,
    ) -> Result<(&'program Expr, Option<&'program str>), SpecError> {
        let mut variable = None;
        for _ in 0..MAX_ALIASES {
            let Expr::Name(name) = expr else {
                return Ok((expr, variable));
            };
            expr = self
                .variables
                .get(name)
                .ok_or_else(|| SpecError::new(format!("`{name}` is used but never assigned")))?;
            variable = Some(name.as_str());
        }
        Err(SpecError::new("names refer to each other in a loop"))
    }

    fn reference(&self, expr: &Expr, out: &mut Output) -> Result<String, SpecError> {
        let (target, variable) = self.resolve(expr)?;
        let id = if let Some(name) = variable {
            if !out.built.insert(name.to_owned()) {
                return Ok(name.to_owned());
            }
            name.to_owned()
        } else {
            out.inline += 1;
            format!("#{}", out.inline)
        };
        self.build(target, &id, out)?;
        Ok(id)
    }

    fn build(&self, expr: &Expr, id: &str, out: &mut Output) -> Result<(), SpecError> {
        let Expr::Call(name, arguments) = expr else {
            return Err(SpecError::new(format!(
                "`{id}` must be a component ({NAMES})"
            )));
        };
        let Some(params) = signature(name) else {
            return Err(SpecError::new(format!(
                "`{id}` uses `{name}`, which is not in the catalog ({NAMES})"
            )));
        };
        let accepted = || {
            params
                .iter()
                .map(|(param, _)| *param)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let place = out.components.len();
        out.components.push(Value::Null);
        let mut fields = Map::new();
        fields.insert("id".to_owned(), Value::String(id.to_owned()));
        fields.insert("component".to_owned(), Value::String(name.clone()));
        let mut position = 0;
        for (key, argument) in arguments {
            let (param, kind) = if let Some(key) = key {
                params
                    .iter()
                    .find(|(param, _)| param == key)
                    .ok_or_else(|| {
                        SpecError::new(format!("{name} has no argument `{key}` ({})", accepted()))
                    })?
            } else {
                let param = params.get(position).ok_or_else(|| {
                    SpecError::new(format!(
                        "{name} takes at most {} arguments ({})",
                        params.len(),
                        accepted()
                    ))
                })?;
                position += 1;
                param
            };
            if matches!(argument, Expr::Name(skipped) if skipped == "_") {
                continue;
            }
            fields.insert((*param).to_owned(), self.prop(name, *kind, argument, out)?);
        }
        out.components[place] = Value::Object(fields);
        Ok(())
    }

    fn prop(
        &self,
        component: &str,
        kind: Param,
        argument: &Expr,
        out: &mut Output,
    ) -> Result<Value, SpecError> {
        match kind {
            Param::Plain => self.value(argument),
            Param::Child => self.reference(argument, out).map(Value::String),
            Param::Action => self.action(argument),
            Param::Rows => plain_path(argument).map(Value::String),
            Param::Columns => {
                let mut columns = self.value(argument)?;
                for column in columns.as_array_mut().into_iter().flatten() {
                    let bound = column
                        .pointer("/path/@path")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    if let (Some(path), Some(fields)) = (bound, column.as_object_mut()) {
                        fields.insert("path".to_owned(), Value::String(path));
                    }
                }
                Ok(columns)
            }
            Param::Children => {
                if let Expr::Call(function, arguments) = argument
                    && function == "_template"
                {
                    let [(_, list), (_, item)] = arguments.as_slice() else {
                        return Err(SpecError::new(
                            "_template takes a list path and a component",
                        ));
                    };
                    return Ok(json!({
                        "componentId": self.reference(item, out)?,
                        "path": plain_path(list)?,
                    }));
                }
                match self.resolve(argument)?.0 {
                    Expr::List(items) => items
                        .iter()
                        .map(|item| self.reference(item, out).map(Value::String))
                        .collect::<Result<Vec<_>, _>>()
                        .map(Value::Array),
                    Expr::Call(..) => Ok(json!([self.reference(argument, out)?])),
                    _ => Err(SpecError::new(format!(
                        "{component} children must be a list like [a, b]"
                    ))),
                }
            }
        }
    }

    fn action(&self, argument: &Expr) -> Result<Value, SpecError> {
        let shape = || SpecError::new(r#"an action must be Event("name", {context})"#);
        let Expr::Call(function, arguments) = self.resolve(argument)?.0 else {
            return Err(shape());
        };
        if function != "Event" {
            return Err(shape());
        }
        let mut event = Map::new();
        let mut position = 0;
        for (key, value) in arguments {
            let key = match key.as_deref() {
                Some(key @ ("name" | "context" | "userMessage")) => key,
                Some(_) => return Err(shape()),
                None => {
                    let key = ["name", "context"].get(position).ok_or_else(shape)?;
                    position += 1;
                    key
                }
            };
            if key == "context"
                && let Expr::Map(entries) = self.resolve(value)?.0
            {
                let order = entries.iter().map(|(name, _)| json!(name)).collect();
                event.insert("order".to_owned(), Value::Array(order));
            }
            event.insert(key.to_owned(), self.value(value)?);
        }
        Ok(json!({ "event": event }))
    }

    fn value(&self, expr: &Expr) -> Result<Value, SpecError> {
        match expr {
            Expr::Literal(value) => Ok(value.clone()),
            Expr::Path(path) => Ok(json!({ "@path": path })),
            Expr::List(items) => items
                .iter()
                .map(|item| self.value(item))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            Expr::Map(entries) => entries
                .iter()
                .map(|(key, item)| Ok((key.clone(), self.value(item)?)))
                .collect::<Result<Map<_, _>, SpecError>>()
                .map(Value::Object),
            Expr::Name(_) => self.value(self.resolve(expr)?.0),
            Expr::Call(function, _) => Err(SpecError::new(format!(
                "`{function}(...)` is not available here"
            ))),
        }
    }
}

fn plain_path(expr: &Expr) -> Result<String, SpecError> {
    match expr {
        Expr::Path(path) | Expr::Literal(Value::String(path)) => Ok(path.clone()),
        _ => Err(SpecError::new("a list path must be written like $/items")),
    }
}

pub fn compile_express(text: &str) -> Result<Message, SpecError> {
    let mut parser = Parser {
        tokens: tokens(text)?,
        at: 0,
    };
    let mut program = Program::default();
    let mut data = Vec::new();
    for statement in parser.program()? {
        match statement {
            Statement::Assign(name, expr) => {
                program.variables.insert(name, expr);
            }
            Statement::Data(path, expr) => data.push((path, expr)),
            Statement::Bare => {}
        }
    }
    if !program.variables.contains_key(ROOT) {
        return Err(SpecError::new(format!("nothing is assigned to `{ROOT}`")));
    }
    let mut out = Output::default();
    program.reference(&Expr::Name(ROOT.to_owned()), &mut out)?;
    let mut model = Document::default();
    for (path, expr) in &data {
        let pointer = match path.as_str() {
            "" | "/" => String::new(),
            _ => format!("/{}", path.trim_start_matches('/')),
        };
        model.set(&pointer, program.value(expr)?);
    }
    let components = out
        .components
        .into_iter()
        .map(Component::from_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Message::CreateSurface {
        surface_id: SURFACE.to_owned(),
        components,
        data_model: (!data.is_empty()).then(|| model.model().clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::super::document::parse_messages;
    use super::*;

    fn document(messages: impl IntoIterator<Item = Message>) -> Document {
        let mut document = Document::default();
        for message in messages {
            document.apply(message);
        }
        document
    }

    #[test]
    fn the_compact_form_compiles_to_the_same_document_as_its_json() {
        let compact = r#"
# inline calls get ids of their own
root = Column([title, Row([name, save]), Column(_template($/panes, pane))])
title = Text("Deploy", "heading")
name = TextField($/form/name, "Name")
save = Button("Save", Event("save", {name: $/form/name}), variant="primary")
pane = Badge($state, "accent")
$/form/name = "staging"
$/panes = [{state: "idle"}]
"#;
        let json = r##"{"components":[
            {"id":"root","component":"Column","children":["title","#1","#2"]},
            {"id":"title","component":"Text","text":"Deploy","variant":"heading"},
            {"id":"#1","component":"Row","children":["name","save"]},
            {"id":"name","component":"TextField","value":{"@path":"/form/name"},"label":"Name"},
            {"id":"save","component":"Button","label":"Save","variant":"primary",
             "action":{"event":{"name":"save","context":{"name":{"@path":"/form/name"}},"order":["name"]}}},
            {"id":"#2","component":"Column","children":{"componentId":"pane","path":"/panes"}},
            {"id":"pane","component":"Badge","label":{"@path":"state"},"tone":"accent"}
        ],"dataModel":{"form":{"name":"staging"},"panes":[{"state":"idle"}]}}"##;
        let compiled = document([compile_express(compact).expect("the compact form compiles")]);
        assert_eq!(compiled.problems(), Vec::<String>::new());
        assert_eq!(
            compiled,
            document(parse_messages(json).expect("the json parses"))
        );
    }

    #[test]
    fn a_bad_compact_block_says_what_to_fix() {
        let error = |text| {
            compile_express(text)
                .expect_err("the block is wrong")
                .to_string()
        };
        assert_eq!(
            error(r#"root = Text("a", colour="red")"#),
            "Text has no argument `colour` (text, variant, tone)"
        );
        assert_eq!(
            error("root = Column([missing])"),
            "`missing` is used but never assigned"
        );
        assert_eq!(
            error("title = Text(\"a\")\nroot = Column([title"),
            "line 2: `]` is missing"
        );
        assert_eq!(
            error(r#"root = Badge("ok", "green")"#),
            "component `root` (Badge): unknown variant `green`, expected one of `neutral`, `primary`, `danger`, `warning`, `success`, `accent`"
        );
    }
}
