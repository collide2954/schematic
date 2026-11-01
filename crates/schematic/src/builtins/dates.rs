use std::rc::Rc;

use super::collections::expects;
use super::native;
use super::strings::as_str;
use crate::array::{self, Array};
use crate::date::{self, Unit};
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "date", Arity::Between(1, 3), date_fn);
    native(globals, "date?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Date(_)))));
    native(globals, "today", Arity::Exact(0), |_| Ok(Value::Date(today())));
    native(globals, "year", Arity::Exact(1), |args| part("year", &args[0], |d| date::to_civil(d).0));
    native(globals, "month", Arity::Exact(1), |args| part("month", &args[0], |d| date::to_civil(d).1 as i64));
    native(globals, "day", Arity::Exact(1), |args| part("day", &args[0], |d| date::to_civil(d).2 as i64));
    native(globals, "weekday", Arity::Exact(1), |args| part("weekday", &args[0], |d| date::weekday(d) as i64));
    native(globals, "day-of-year", Arity::Exact(1), |args| part("day-of-year", &args[0], |d| date::day_of_year(d) as i64));
    native(globals, "quarter", Arity::Exact(1), |args| part("quarter", &args[0], |d| (date::to_civil(d).1 as i64 - 1) / 3 + 1));
    native(globals, "days-in-month", Arity::Exact(1), |args| {
        part("days-in-month", &args[0], |d| {
            let (year, month, _) = date::to_civil(d);
            date::days_in_month(year, month) as i64
        })
    });
    native(globals, "leap-year?", Arity::Exact(1), leap_year);
    native(globals, "date-format", Arity::Exact(2), |args| {
        let pattern = as_str("date-format", &args[1])?;
        each("date-format", &args[0], &|d| Ok(Value::string(&date::format(d, pattern))))
    });
    native(globals, "date-trunc", Arity::Exact(2), |args| {
        let unit = unit("date-trunc", &args[1])?;
        each("date-trunc", &args[0], &|d| Ok(Value::Date(date::truncate(d, unit))))
    });
    native(globals, "add-months", Arity::Exact(2), |args| shift_months("add-months", args, 1));
    native(globals, "add-years", Arity::Exact(2), |args| shift_months("add-years", args, 12));
    native(globals, "date-range", Arity::Between(2, 3), date_range);
}

fn out_of_range() -> Error {
    Error::new("value-error", "date out of range")
}

fn today() -> i32 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    seconds.div_euclid(86_400) as i32
}

fn each(name: &str, subject: &Value, f: &dyn Fn(i32) -> Result<Value, Error>) -> Result<Value, Error> {
    array::unary(subject, &|value| match value {
        Value::Date(d) => f(*d),
        other => Err(expects(name, "a date", other)),
    })
}

fn part(name: &str, subject: &Value, f: fn(i32) -> i64) -> Result<Value, Error> {
    each(name, subject, &|d| Ok(Value::Int(f(d))))
}

fn unit(name: &str, value: &Value) -> Result<Unit, Error> {
    let unit = match value {
        Value::Keyword(k) => match &*k.name() {
            "week" => Some(Unit::Week),
            "month" => Some(Unit::Month),
            "quarter" => Some(Unit::Quarter),
            "year" => Some(Unit::Year),
            _ => None,
        },
        _ => None,
    };
    unit.ok_or_else(|| expects(name, ":week, :month, :quarter, or :year", value))
}

fn parse_dates(text: &Value, pattern: Option<&str>) -> Result<Value, Error> {
    array::unary(text, &|value| {
        if let Value::Date(_) = value {
            return Ok(value.clone());
        }
        let s = as_str("date", value)?;
        let parsed = match pattern {
            None => date::parse_iso(s),
            Some(pattern) => date::parse(s, pattern),
        };
        parsed.map(Value::Date).ok_or_else(|| {
            let message = match pattern {
                None => format!("date cannot read {s:?} as a date"),
                Some(pattern) => format!("date cannot read {s:?} with the pattern {pattern:?}"),
            };
            Error::new("value-error", message)
        })
    })
}

fn date_fn(args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Int(y), Value::Int(m), Value::Int(d)] => {
            let parts = u32::try_from(*m).ok().zip(u32::try_from(*d).ok());
            parts
                .and_then(|(m, d)| date::from_civil(*y, m, d))
                .map(Value::Date)
                .ok_or_else(|| Error::new("value-error", format!("{y:04}-{m:02}-{d:02} is not a valid date")))
        }
        [text] => parse_dates(text, None),
        [text, pattern] => parse_dates(text, Some(as_str("date", pattern)?)),
        _ => Err(expects("date", "a year, month, and day", &args[0])),
    }
}

fn leap_year(args: &[Value]) -> Result<Value, Error> {
    array::unary(&args[0], &|value| match value {
        Value::Date(d) => Ok(Value::Bool(date::is_leap(date::to_civil(*d).0))),
        Value::Int(year) => Ok(Value::Bool(date::is_leap(*year))),
        other => Err(expects("leap-year?", "a date or a year", other)),
    })
}

fn shift_months(name: &str, args: &[Value], scale: i64) -> Result<Value, Error> {
    let count = match &args[1] {
        Value::Int(n) => *n,
        other => return Err(expects(name, "an integer count", other)),
    };
    each(name, &args[0], &|d| date::add_months(d, count * scale).map(Value::Date).ok_or_else(out_of_range))
}

fn date_range(args: &[Value]) -> Result<Value, Error> {
    let (start, end) = match (&args[0], &args[1]) {
        (Value::Date(start), Value::Date(end)) => (*start, *end),
        (Value::Date(_), other) | (other, _) => return Err(expects("date-range", "dates", other)),
    };
    let step = match args.get(2) {
        None => None,
        Some(Value::Keyword(k)) if &*k.name() == "day" => None,
        Some(value) => Some(unit("date-range", value)?),
    };
    let mut out = Array::new();
    for k in 0i64.. {
        let current = match step {
            None => start as i64 + k,
            Some(Unit::Week) => start as i64 + 7 * k,
            Some(Unit::Month) => date::add_months(start, k).ok_or_else(out_of_range)? as i64,
            Some(Unit::Quarter) => date::add_months(start, 3 * k).ok_or_else(out_of_range)? as i64,
            Some(Unit::Year) => date::add_months(start, 12 * k).ok_or_else(out_of_range)? as i64,
        };
        if current > end as i64 {
            break;
        }
        out.push_mut(Value::Date(current as i32))?;
    }
    Ok(Value::Array(Rc::new(out)))
}
