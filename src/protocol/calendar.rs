use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(uniffi::Record))]
pub struct Calendar {
    /// Optional weekday specification (Mon,Tue..Fri)
    weekdays: Option<Vec<Weekday>>,
    /// Year component (can be * or specific years)
    year: TimeComponent,
    /// Month component (can be * or 1-12)
    month: TimeComponent,
    /// Day component (can be * or 1-31)
    day: TimeComponent,
    /// Hour component (can be * or 0-23)
    hour: TimeComponent,
    /// Minute component (can be * or 0-59)
    minute: TimeComponent,
    /// Second component (can be * or 0-59)
    second: TimeComponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(uniffi::Enum))]
pub enum TimeComponent {
    /// Matches any value (*)
    Any,
    /// Matches specific values
    Values(Vec<u32>),
    /// Matches a range of values with optional step
    Range {
        start: u32,
        end: u32,
        step: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(uniffi::Enum))]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

#[derive(Debug, Error)]
#[cfg_attr(feature = "bindings", derive(uniffi::Error))]
pub enum CalendarError {
    #[error("Invalid calendar format")]
    InvalidFormat,
    #[error("Invalid weekday: {0}")]
    InvalidWeekday(String),
    #[error("Invalid time component: {0}")]
    InvalidTimeComponent(String),
    #[error("Invalid range: {start} > {end}")]
    InvalidRange { start: u32, end: u32 },
}

impl FromStr for Weekday {
    type Err = CalendarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_lower = s.to_lowercase();
        match s_lower.as_str() {
            "mon" | "monday" => Ok(Weekday::Mon),
            "tue" | "tuesday" => Ok(Weekday::Tue),
            "wed" | "wednesday" => Ok(Weekday::Wed),
            "thu" | "thursday" => Ok(Weekday::Thu),
            "fri" | "friday" => Ok(Weekday::Fri),
            "sat" | "saturday" => Ok(Weekday::Sat),
            "sun" | "sunday" => Ok(Weekday::Sun),
            _ => Err(CalendarError::InvalidWeekday(s_lower)),
        }
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Weekday::Mon => "Mon",
                Weekday::Tue => "Tue",
                Weekday::Wed => "Wed",
                Weekday::Thu => "Thu",
                Weekday::Fri => "Fri",
                Weekday::Sat => "Sat",
                Weekday::Sun => "Sun",
            }
        )
    }
}

impl FromStr for TimeComponent {
    type Err = CalendarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "*" {
            return Ok(TimeComponent::Any);
        }

        if s.contains("..") {
            let parts: Vec<&str> = s.split("..").collect();
            if parts.len() != 2 {
                return Err(CalendarError::InvalidTimeComponent(s.to_string()));
            }

            let start = parts[0].parse().map_err(|_| {
                CalendarError::InvalidTimeComponent(format!("Invalid start value: {}", parts[0]))
            })?;
            
            let (end, step) = if parts[1].contains('/') {
                let step_parts: Vec<&str> = parts[1].split('/').collect();
                if step_parts.len() != 2 {
                    return Err(CalendarError::InvalidTimeComponent(s.to_string()));
                }
                let end = step_parts[0].parse().map_err(|_| {
                    CalendarError::InvalidTimeComponent(format!("Invalid end value: {}", step_parts[0]))
                })?;
                let step = step_parts[1].parse().map_err(|_| {
                    CalendarError::InvalidTimeComponent(format!("Invalid step value: {}", step_parts[1]))
                })?;
                (end, Some(step))
            } else {
                let end = parts[1].parse().map_err(|_| {
                    CalendarError::InvalidTimeComponent(format!("Invalid end value: {}", parts[1]))
                })?;
                (end, None)
            };

            if start > end {
                return Err(CalendarError::InvalidRange { start, end });
            }

            Ok(TimeComponent::Range { start, end, step })
        } else if s.contains(',') {
            let values: Result<Vec<u32>, _> = s
                .split(',')
                .map(|v| {
                    v.parse().map_err(|_| {
                        CalendarError::InvalidTimeComponent(format!("Invalid value: {}", v))
                    })
                })
                .collect();
            Ok(TimeComponent::Values(values?))
        } else {
            let value = s.parse().map_err(|_| {
                CalendarError::InvalidTimeComponent(format!("Invalid value: {}", s))
            })?;
            Ok(TimeComponent::Values(vec![value]))
        }
    }
}

impl fmt::Display for TimeComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeComponent::Any => write!(f, "*"),
            TimeComponent::Values(values) => {
                let s = values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "{}", s)
            }
            TimeComponent::Range { start, end, step } => {
                write!(f, "{}..{}", start, end)?;
                if let Some(step) = step {
                    write!(f, "/{}", step)?;
                }
                Ok(())
            }
        }
    }
}

impl Calendar {
    pub fn minutely() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Any,
            day: TimeComponent::Any,
            hour: TimeComponent::Any,
            minute: TimeComponent::Any,
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn hourly() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Any,
            day: TimeComponent::Any,
            hour: TimeComponent::Any,
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn daily() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Any,
            day: TimeComponent::Any,
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn weekly() -> Self {
        Calendar {
            weekdays: Some(vec![Weekday::Mon]),
            year: TimeComponent::Any,
            month: TimeComponent::Any,
            day: TimeComponent::Any,
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn monthly() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Any,
            day: TimeComponent::Values(vec![1]),
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn yearly() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Values(vec![1]),
            day: TimeComponent::Values(vec![1]),
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn quarterly() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Values(vec![1, 4, 7, 10]),
            day: TimeComponent::Values(vec![1]),
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    pub fn semiannually() -> Self {
        Calendar {
            weekdays: None,
            year: TimeComponent::Any,
            month: TimeComponent::Values(vec![1, 7]),
            day: TimeComponent::Values(vec![1]),
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        }
    }

    fn validate_time_component(comp: &TimeComponent, max: u32) -> Result<(), CalendarError> {
        match comp {
            TimeComponent::Any => Ok(()),
            TimeComponent::Values(values) => {
                for &v in values {
                    if v > max {
                        return Err(CalendarError::InvalidTimeComponent(
                            format!("Value {} exceeds maximum {}", v, max)
                        ));
                    }
                }
                Ok(())
            }
            TimeComponent::Range { start, end, .. } => {
                if *start > max || *end > max {
                    return Err(CalendarError::InvalidTimeComponent(
                        format!("Range {}-{} exceeds maximum {}", start, end, max)
                    ));
                }
                Ok(())
            }
        }
    }
}

impl FromStr for Calendar {
    type Err = CalendarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Handle special keywords
        match s.to_lowercase().as_str() {
            "minutely" => return Ok(Calendar::minutely()),
            "hourly" => return Ok(Calendar::hourly()),
            "daily" => return Ok(Calendar::daily()),
            "monthly" => return Ok(Calendar::monthly()),
            "weekly" => return Ok(Calendar::weekly()),
            "yearly" | "annually" => return Ok(Calendar::yearly()),
            "quarterly" => return Ok(Calendar::quarterly()),
            "semiannually" => return Ok(Calendar::semiannually()),
            _ => {}
        }

        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CalendarError::InvalidFormat);
        }

        let mut weekdays = None;
        let mut date_parts = None;
        let mut time_parts = None;

        for part in parts {
            if part.contains(':') {
                // Check for invalid separators within the time part
                if part.contains('.') {
                    return Err(CalendarError::InvalidFormat);
                }
                // Time part
                time_parts = Some(part.split(':').collect::<Vec<_>>());
            } else if part.contains('-') || part == "*" {
                // Date part
                date_parts = Some(part.split('-').collect::<Vec<_>>());
            } else if part.contains('/') || part.contains('.') {
                // Explicitly reject parts with slashes or periods before trying weekday parsing
                return Err(CalendarError::InvalidFormat);
            } else {
                // Try parsing as weekday part (single, list, or range)
                let weekday_list = if part.contains("..") {
                    // Parse range
                    let range: Vec<&str> = part.split("..").collect();
                    if range.len() != 2 {
                        return Err(CalendarError::InvalidFormat);
                    }
                    let start = Weekday::from_str(range[0])?;
                    let end = Weekday::from_str(range[1])?;
                    (start as u8..=end as u8)
                        .map(|d| unsafe { std::mem::transmute(d) })
                        .collect()
                } else {
                    // Parse list or single
                    part.split(',')
                        .map(Weekday::from_str)
                        .collect::<Result<Vec<_>, _>>()?
                };
                weekdays = Some(weekday_list);
            }
        }

        // Parse date components
        let (year, month, day) = match date_parts {
            Some(parts) => {
                let year = parts.get(0).unwrap_or(&"*").parse()?;
                let month = parts.get(1).unwrap_or(&"*").parse()?;
                let day = parts.get(2).unwrap_or(&"*").parse()?;
                (year, month, day)
            }
            None => (TimeComponent::Any, TimeComponent::Any, TimeComponent::Any),
        };

        // Parse time components
        let (hour, minute, second) = match time_parts {
            Some(parts) => {
                let hour: TimeComponent = parts.get(0).unwrap_or(&"*").parse()?;
                Calendar::validate_time_component(&hour, 23)?;
                
                let minute: TimeComponent = parts.get(1).unwrap_or(&"*").parse()?;
                Calendar::validate_time_component(&minute, 59)?;
                
                let second: TimeComponent = parts.get(2).unwrap_or(&"0").parse()?;
                Calendar::validate_time_component(&second, 59)?;
                
                (hour, minute, second)
            }
            None => (
                TimeComponent::Any,
                TimeComponent::Any,
                TimeComponent::Values(vec![0]),
            ),
        };

        Ok(Calendar {
            weekdays,
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }
}

impl fmt::Display for Calendar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref weekdays) = self.weekdays {
            let weekday_str = weekdays
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
                .join(",");
            write!(f, "{} ", weekday_str)?;
        }

        // Always include date part
        write!(f, "{}-{}-{} ", self.year, self.month, self.day)?;

        // Time part
        write!(f, "{}:{}:{}", self.hour, self.minute, self.second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_weekdays() {
        let cal: Calendar = "Mon,Wed,Fri *-*-* 00:00:00".parse().unwrap();
        assert_eq!(
            cal.weekdays,
            Some(vec![Weekday::Mon, Weekday::Wed, Weekday::Fri])
        );
    }

    #[test]
    fn test_parse_weekday_range() {
        let cal: Calendar = "Mon..Fri *-*-* 00:00:00".parse().unwrap();
        assert_eq!(
            cal.weekdays,
            Some(vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri
            ])
        );
    }

    #[test]
    fn test_parse_time_component() {
        assert_eq!(TimeComponent::from_str("*").unwrap(), TimeComponent::Any);
        assert_eq!(
            TimeComponent::from_str("1,2,3").unwrap(),
            TimeComponent::Values(vec![1, 2, 3])
        );
        assert_eq!(
            TimeComponent::from_str("1..5").unwrap(),
            TimeComponent::Range {
                start: 1,
                end: 5,
                step: None
            }
        );
        assert_eq!(
            TimeComponent::from_str("1..10/2").unwrap(),
            TimeComponent::Range {
                start: 1,
                end: 10,
                step: Some(2)
            }
        );
    }

    #[test]
    fn test_parse_special_keywords() {
        assert_eq!(Calendar::from_str("minutely").unwrap(), Calendar::minutely());
        assert_eq!(Calendar::from_str("hourly").unwrap(), Calendar::hourly());
        assert_eq!(Calendar::from_str("daily").unwrap(), Calendar::daily());
        assert_eq!(Calendar::from_str("weekly").unwrap(), Calendar::weekly());
        assert_eq!(Calendar::from_str("monthly").unwrap(), Calendar::monthly());
        assert_eq!(Calendar::from_str("yearly").unwrap(), Calendar::yearly());
        assert_eq!(
            Calendar::from_str("quarterly").unwrap(),
            Calendar::quarterly()
        );
        assert_eq!(
            Calendar::from_str("semiannually").unwrap(),
            Calendar::semiannually()
        );
    }

    #[test]
    fn test_format_calendar() {
        let cal = Calendar {
            weekdays: Some(vec![Weekday::Mon, Weekday::Wed, Weekday::Fri]),
            year: TimeComponent::Any,
            month: TimeComponent::Any,
            day: TimeComponent::Any,
            hour: TimeComponent::Values(vec![0]),
            minute: TimeComponent::Values(vec![0]),
            second: TimeComponent::Values(vec![0]),
        };
        assert_eq!(cal.to_string(), "Mon,Wed,Fri *-*-* 0:0:0");
    }

    #[test]
    fn test_invalid_formats() {
        // Invalid weekday
        assert!(matches!(
            Calendar::from_str("Foo *-*-* 00:00:00"),
            Err(CalendarError::InvalidWeekday(s)) if s == "foo"
        ));

        // Invalid time component
        assert!(matches!(
            Calendar::from_str("Mon *-*-* 25:00:00"),
            Err(CalendarError::InvalidTimeComponent(_))
        ));

        // Invalid range (start > end)
        assert!(matches!(
            Calendar::from_str("Mon *-*-* 10..5:00:00"),
            Err(CalendarError::InvalidFormat)
        ));

        // Empty string
        assert!(matches!(
            Calendar::from_str(""),
            Err(CalendarError::InvalidFormat)
        ));

        // Invalid date format
        assert!(matches!(
            Calendar::from_str("2023/12/25"),
            Err(CalendarError::InvalidFormat)
        ));

        // Invalid time format
        assert!(matches!(
            Calendar::from_str("*-*-* 10.30.00"),
            Err(CalendarError::InvalidFormat)
        ));

        // Invalid weekday range
        assert!(matches!(
            Calendar::from_str("Mon..Invalid *-*-* 00:00:00"),
            Err(CalendarError::InvalidFormat),
        ));

        // Invalid step value
        assert!(matches!(
            Calendar::from_str("*-*-* 1..10/invalid:00:00"),
            Err(CalendarError::InvalidFormat)
        ));
    }
} 