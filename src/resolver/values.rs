use std::collections::{BTreeMap, BTreeSet};

pub(super) type Variables = BTreeMap<String, String>;

pub(super) fn cascade_variables(inherited: &Variables, local: &BTreeMap<&str, &str>) -> Variables {
    let mut result = inherited.clone();
    for &name in local.keys() {
        if let Some(value) = compute_variable(
            name,
            local,
            inherited,
            &mut Vec::new(),
            &mut BTreeSet::new(),
        ) {
            result.insert(name.to_owned(), value);
        } else {
            result.remove(name);
        }
    }
    result
}

fn compute_variable<'a>(
    name: &'a str,
    local: &BTreeMap<&'a str, &'a str>,
    inherited: &Variables,
    stack: &mut Vec<&'a str>,
    cycles: &mut BTreeSet<&'a str>,
) -> Option<String> {
    match local.get(name).copied() {
        None | Some("inherit" | "unset") => inherited.get(name).cloned(),
        Some("initial") => None,
        Some(value) => {
            if let Some(index) = stack.iter().position(|entry| *entry == name) {
                cycles.extend(stack[index..].iter().copied());
                return None;
            }
            stack.push(name);
            let resolved = substitute_variables(value, &mut |dependency| {
                if let Some((key, _)) = local.get_key_value(dependency) {
                    compute_variable(key, local, inherited, stack, cycles)
                } else {
                    inherited.get(dependency).cloned()
                }
            });
            stack.pop();
            if cycles.contains(name) {
                None
            } else {
                resolved
            }
        }
    }
}

pub(super) fn substitute_variables(
    value: &str,
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Option<String> {
    let mut result = String::new();
    let mut rest = value;
    while let Some((index, _)) = unquoted(rest).find(|(index, token)| {
        *token == 'v'
            && rest[*index..].starts_with("var(")
            && rest[..*index]
                .chars()
                .next_back()
                .is_none_or(|ch| !ch.is_alphanumeric() && ch != '-' && ch != '_')
    }) {
        result.push_str(&rest[..index]);
        let argument = &rest[index + 4..];
        let end = tokens(argument).find_map(|(index, token)| (token == ')').then_some(index))?;
        let argument_list = &argument[..end];
        let comma =
            tokens(argument_list).find_map(|(index, token)| (token == ',').then_some(index));
        let name = argument_list[..comma.unwrap_or(argument_list.len())].trim();
        if !name.starts_with("--") {
            return None;
        }
        let replacement = lookup(name).or_else(|| {
            comma.and_then(|comma| substitute_variables(argument_list[comma + 1..].trim(), lookup))
        })?;
        result.push_str(&replacement);
        rest = &argument[end + 1..];
    }
    result.push_str(rest);
    Some(result)
}

/// Top-level characters, excluding quoted text and nested function arguments.
fn tokens(value: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut depth = 0usize;
    unquoted(value).filter(move |&(_, ch)| {
        if ch == '(' {
            depth += 1;
            return false;
        }
        if ch == ')' && depth > 0 {
            depth -= 1;
            return false;
        }
        depth == 0
    })
}

fn unquoted(value: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut quote = None;
    let mut escaped = false;
    value.char_indices().filter(move |&(_, ch)| {
        if escaped {
            escaped = false;
            return false;
        }
        if ch == '\\' {
            escaped = true;
            return false;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            return false;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            return false;
        }
        true
    })
}

pub(super) fn value_parts(value: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    tokens(value)
        .filter(|(_, ch)| ch.is_ascii_whitespace())
        .map(|(index, _)| index)
        .chain(std::iter::once(value.len()))
        .filter_map(move |end| {
            let part = &value[start..end];
            start = end + 1;
            (!part.is_empty()).then_some(part)
        })
}

#[derive(Clone, Copy)]
pub(super) struct LengthContext {
    pub em: f32,
    pub rem: f32,
    pub viewport: Option<(f32, f32)>,
    pub percent: Option<f32>,
}

impl Default for LengthContext {
    fn default() -> Self {
        Self {
            em: 16.0,
            rem: 16.0,
            viewport: None,
            percent: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Quantity {
    Number(f32),
    Length(f32),
}

impl Quantity {
    fn magnitude(self) -> f32 {
        match self {
            Self::Number(value) | Self::Length(value) => value,
        }
    }

    fn map(self, f: impl FnOnce(f32) -> f32) -> Self {
        match self {
            Self::Number(value) => Self::Number(f(value)),
            Self::Length(value) => Self::Length(f(value)),
        }
    }

    fn same_unit(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    fn math(self, operator: char, rhs: Self) -> Option<Self> {
        match (operator, self, rhs) {
            ('+' | '-', left, right) if left.same_unit(right) => Some(left.map(|value| {
                if operator == '+' {
                    value + right.magnitude()
                } else {
                    value - right.magnitude()
                }
            })),
            ('*', Self::Number(left), right) => Some(right.map(|value| left * value)),
            ('*', left, Self::Number(right)) => Some(left.map(|value| value * right)),
            ('/', left, Self::Number(right)) if right != 0.0 => {
                Some(left.map(|value| value / right))
            }
            _ => None,
        }
    }
}

impl LengthContext {
    pub(super) fn pixels(self, value: &str) -> Option<f32> {
        match self.quantity(value.trim())? {
            Quantity::Length(value) if value.is_finite() => Some(value),
            Quantity::Number(0.0) => Some(0.0),
            _ => None,
        }
    }

    fn quantity(self, value: &str) -> Option<Quantity> {
        if let Ok(number) = value.parse() {
            return Some(Quantity::Number(number));
        }
        for operators in [['+', '-'], ['*', '/']] {
            let operator = tokens(value)
                .filter(|&(index, ch)| {
                    operators.contains(&ch)
                        && index > 0
                        && (!matches!(ch, '+' | '-')
                            || (value.as_bytes()[index - 1].is_ascii_whitespace()
                                && value
                                    .as_bytes()
                                    .get(index + 1)
                                    .is_some_and(u8::is_ascii_whitespace)))
                })
                .last();
            if let Some((index, operator)) = operator {
                let left = self.quantity(value[..index].trim())?;
                let right = self.quantity(value[index + 1..].trim())?;
                return left.math(operator, right);
            }
        }
        if let Some(inner) = value
            .strip_prefix('(')
            .and_then(|input| input.strip_suffix(')'))
        {
            return self.quantity(inner.trim());
        }
        if let Some((name, inner)) = value.split_once('(') {
            let inner = inner.strip_suffix(')')?;
            return self.function(name, inner);
        }

        let unit = value[..].rfind(|ch: char| ch.is_ascii_digit() || ch == '.')? + 1;
        let (number, suffix) = value.split_at(unit);
        let number: f32 = number.parse().ok()?;
        let scale = match suffix {
            "px" => 1.0,
            "em" => self.em,
            "rem" => self.rem,
            "vw" | "svw" => self.viewport?.0 / 100.0,
            "vh" | "svh" => self.viewport?.1 / 100.0,
            "%" => self.percent? / 100.0,
            _ => return None,
        };
        Some(Quantity::Length(number * scale))
    }

    fn function(self, name: &str, inner: &str) -> Option<Quantity> {
        if name == "calc" {
            return self.quantity(inner.trim());
        }
        let mut start = 0;
        let quantities = tokens(inner)
            .filter(|(_, ch)| *ch == ',')
            .map(|(index, _)| index)
            .chain(std::iter::once(inner.len()))
            .map(|end| {
                let value = self.quantity(inner[start..end].trim());
                start = end + 1;
                value
            })
            .collect::<Option<Vec<_>>>()?;
        let first = *quantities.first()?;
        if !quantities.iter().all(|value| first.same_unit(*value)) {
            return None;
        }
        let result = match (name, quantities.as_slice()) {
            ("min", quantities) => quantities
                .iter()
                .map(|value| value.magnitude())
                .reduce(f32::min)?,
            ("max", quantities) => quantities
                .iter()
                .map(|value| value.magnitude())
                .reduce(f32::max)?,
            ("clamp", [min, value, max]) => {
                value.magnitude().min(max.magnitude()).max(min.magnitude())
            }
            _ => return None,
        };
        Some(first.map(|_| result))
    }
}
