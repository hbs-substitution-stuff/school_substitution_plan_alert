use std::fmt::Display;
use std::fmt::Formatter;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Extracts the text from the rows and cells in the json that gets outputted by tabula
pub fn parse(content: &str) -> Result<Vec<Vec<Vec<String>>>, Box<dyn std::error::Error>> {
	let json: Value = serde_json::from_str(content)?;
	let array = json.as_array().ok_or("Json malformed")?;

	let mut tables = Vec::new();
	for entry in array {
		let object = entry.as_object().ok_or("Json malformed")?;
		let data = object.get("data").ok_or("Json data field missing")?;

		let mut table_rows = Vec::new();
		for row in data.as_array().ok_or("Json data missing")? {
			let row: Vec<Cell> = serde_json::from_value(row.clone())?;
			let row = Row {
				row
			};
			table_rows.push(row);
		}
		tables.push(table_rows);
	}

	let mut tables_with_rows_as_text = Vec::new();
	for table_rows in tables {
		let mut rows_as_text = Vec::new();
		for mut row in table_rows {
			rows_as_text.push(row.extract_text());
		}
		tables_with_rows_as_text.push(rows_as_text);
	}

	Ok(tables_with_rows_as_text)
}

/// A row in the substitution table
#[derive(Debug, Deserialize, Serialize)]
struct Row {
	row: Vec<Cell>,
}

impl Row {
	/// Gets the string content of every Cell inside the Row
	pub fn extract_text(&mut self) -> Vec<String> {
		let mut text = Vec::new();
		for cell in &self.row {
			text.push(cell.text.clone());
		}

		text
	}
}

/// A cell in the substitution table
#[derive(Debug, Deserialize, Serialize)]
struct Cell {
	top: f64,
	left: f64,
	width: f64,
	height: f64,
	text: String,
}

impl Display for Cell {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.text)
	}
}