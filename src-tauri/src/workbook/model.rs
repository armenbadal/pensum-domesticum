use serde::{Deserialize, Serialize};

use super::limits::{CURRENT_FORMAT_VERSION, Limits};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Workbook {
    #[serde(rename = "formatVersion")]
    pub(crate) format_version: u8,
    pub(crate) solved: bool,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) teacher: Person,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) student: Option<Student>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) verified: Option<Verified>,
    #[serde(rename = "created-at")]
    pub(crate) created_at: String,
    pub(crate) assignments: Vec<Assignment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Person {
    pub(crate) name: String,
    pub(crate) email: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Student {
    pub(crate) name: String,
    pub(crate) email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) class: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Verified {
    pub(crate) grade: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Assignment {
    pub(crate) problem: Vec<ContentBlock>,
    pub(crate) solution: Vec<ContentBlock>,
    #[serde(
        rename = "teacher-comment",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) teacher_comment: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum ContentBlock {
    Text { content: String },
    Image { url: String },
    Squared { content: MathGridState },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MathGridState {
    pub(crate) version: u8,
    pub(crate) rows: u32,
    pub(crate) columns: u32,
    pub(crate) cells: String,
    pub(crate) borders: Vec<GridBorder>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GridBorder {
    pub(crate) cell: usize,
    pub(crate) edge: GridEdge,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GridEdge {
    Bottom,
    Left,
}

pub(crate) fn validate_workbook(workbook: &Workbook, limits: &Limits) -> Result<(), String> {
    if workbook.format_version != CURRENT_FORMAT_VERSION {
        return Err("Տետրի ZIP ձևաչափի տարբերակը չի աջակցվում։".to_string());
    }
    if !is_valid_date(&workbook.created_at) {
        return Err("Տետրի ստեղծման ամսաթիվը սխալ ձևաչափ ունի։".to_string());
    }
    if workbook.student.is_some() && !workbook.solved {
        return Err("Սովորողի տվյալները թույլատրվում են միայն լուծված տետրում։".to_string());
    }
    if let Some(verified) = &workbook.verified {
        if !workbook.solved {
            return Err("Գնահատականը թույլատրվում է միայն լուծված տետրում։".to_string());
        }
        if !(1..=20).contains(&verified.grade) {
            return Err("Տետրի գնահատականը պետք է լինի 1-ից 20 միջակայքում։".to_string());
        }
    }

    for assignment in &workbook.assignments {
        for block in assignment.problem.iter().chain(&assignment.solution) {
            if let ContentBlock::Squared { content } = block {
                validate_math_grid(content, limits)?;
            }
        }
    }

    Ok(())
}

fn validate_math_grid(grid: &MathGridState, limits: &Limits) -> Result<(), String> {
    if grid.version != 2 {
        return Err("Վանդակավոր դաշտի տարբերակը չի աջակցվում։".to_string());
    }
    let rows = grid.rows as usize;
    let columns = grid.columns as usize;
    if rows == 0 || rows > limits.grid_rows {
        return Err("Վանդակավոր դաշտի տողերի քանակը սահմանից դուրս է։".to_string());
    }
    if columns == 0 || columns > limits.grid_columns {
        return Err("Վանդակավոր դաշտի սյուների քանակը սահմանից դուրս է։".to_string());
    }
    let cell_count = rows
        .checked_mul(columns)
        .ok_or_else(|| "Վանդակավոր դաշտի չափերը չափազանց մեծ են։".to_string())?;

    let lines: Vec<&str> = grid.cells.split('\n').collect();
    if lines.len() != rows {
        return Err("Վանդակավոր դաշտի տողերի քանակը չի համապատասխանում բջիջներին։".to_string());
    }
    for line in lines {
        let characters: Vec<char> = line.chars().collect();
        if characters.len() != columns {
            return Err(
                "Վանդակավոր դաշտի սյուների քանակը չի համապատասխանում բջիջներին։".to_string(),
            );
        }
        if characters
            .iter()
            .any(|character| !is_allowed_grid_character(*character))
        {
            return Err("Վանդակավոր դաշտում անթույլատրելի նիշ կա։".to_string());
        }
    }

    if grid.borders.iter().any(|border| border.cell >= cell_count) {
        return Err("Վանդակավոր դաշտի եզրագիծը սահմանից դուրս է։".to_string());
    }

    Ok(())
}

fn is_allowed_grid_character(character: char) -> bool {
    character == ' '
        || character.is_ascii_digit()
        || matches!(
            character,
            '+' | '-' | '−' | '*' | '/' | '×' | '÷' | '=' | '(' | ')'
        )
}

fn is_valid_date(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let Ok(year) = parts[0].parse::<u32>() else {
        return false;
    };
    let Ok(month) = parts[1].parse::<u32>() else {
        return false;
    };
    let Ok(day) = parts[2].parse::<u32>() else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let days_in_month = match month {
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days_in_month).contains(&day)
}
