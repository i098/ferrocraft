//! `StatBase` formatters of 1.8.9: counters, ticks as a time, centimeters as a distance and tenths.

/// Which `StatBase.simpleStatType`, `timeStatType`, `distanceStatType` or `field_111202_k` a stat uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Number,
    Time,
    Distance,
    Tenth,
}

impl Format {
    pub fn format(self, value: i32) -> String {
        match self {
            Format::Number => number(value),
            Format::Time => time(value),
            Format::Distance => distance(value),
            Format::Tenth => decimal(f64::from(value) * 0.1),
        }
    }
}

/// `NumberFormat.getIntegerInstance(Locale.US)`: groups of three digits.
fn number(value: i32) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        out.push('-');
    }
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    out
}

/// `DecimalFormat("########0.00")`: two places, half-even on the exact binary value.
fn decimal(x: f64) -> String {
    format!("{x:.2}")
}

/// `Double.toString`: plain between 1e-3 and 1e7, else `1.5E10`.
fn java_double(x: f64) -> String {
    let a = x.abs();
    if a == 0.0 || (1e-3..1e7).contains(&a) {
        return format!("{x:?}");
    }
    let sci = format!("{x:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let dot = if mantissa.contains('.') { "" } else { ".0" };
    format!("{mantissa}{dot}E{exp}")
}

/// `StatBase.timeStatType`: ticks as years, days, hours, minutes or seconds.
fn time(ticks: i32) -> String {
    let s = f64::from(ticks) / 20.0;
    let m = s / 60.0;
    let h = m / 60.0;
    let d = h / 24.0;
    let y = d / 365.0;
    if y > 0.5 {
        format!("{} y", decimal(y))
    } else if d > 0.5 {
        format!("{} d", decimal(d))
    } else if h > 0.5 {
        format!("{} h", decimal(h))
    } else if m > 0.5 {
        format!("{} m", decimal(m))
    } else {
        format!("{} s", java_double(s))
    }
}

/// `StatBase.distanceStatType`: centimeters as kilometers, meters or centimeters.
fn distance(cm: i32) -> String {
    let m = f64::from(cm) / 100.0;
    let km = m / 1000.0;
    if km > 0.5 {
        format!("{} km", decimal(km))
    } else if m > 0.5 {
        format!("{} m", decimal(m))
    } else {
        format!("{cm} cm")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every formatter against the real `Stat.format` of 1.8.9, over the thresholds, rounding
    /// ties, large values and negatives.
    #[test]
    fn formatters_match_the_real_client() {
        let mut values: Vec<i32> = (-60..=2400).collect();
        values.extend((0..=2000).map(|n| n * 997));
        values.extend((0..=300).map(|n| n * 50_000 + 17));
        values.extend([
            i32::MIN,
            i32::MIN + 1,
            -1_000_000,
            -199_999,
            -200_000,
            199_999,
            200_000,
            1_000_000,
            i32::MAX - 1,
            i32::MAX,
            12_614_400,
            12_614_401,
            63_072_000,
            63_072_001,
        ]);
        let args: Vec<String> = values.iter().map(i32::to_string).collect();
        for (name, format) in [
            ("number", Format::Number),
            ("time", Format::Time),
            ("distance", Format::Distance),
            ("tenth", Format::Tenth),
        ] {
            let script = format!("statfmt {name} {}\n", args.join(" "));
            let want = oracle::Oracle::get().run(&script).unwrap();
            assert_eq!(want.len(), values.len(), "{name}");
            for (v, w) in values.iter().zip(&want) {
                assert_eq!(&format.format(*v), w, "{name} {v}");
            }
        }
    }
}
