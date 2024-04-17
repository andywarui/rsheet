mod tests;
use rsheet_lib::connect::{Manager, Reader, Writer};
use rsheet_lib::replies::{Reply, CellValue};
use rsheet_lib::cell_runner::CommandRunner;
use rhai::{Engine, RegisterFn, Dynamic};
use std::error::Error;
use log::info;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Data structures
struct Spreadsheet {
    cells: HashMap<String, CellValue>, 
}

impl Spreadsheet {
    fn new() -> Self { 
        Self { cells: HashMap::new() }
    }

    fn set_cell_value(&mut self, cell_name: String, value: CellValue) {
        self.cells.insert(cell_name, value);
    }

    fn get_cell_value(&self, cell_name: &str) -> Option<&CellValue> {
        self.cells.get(cell_name)
    }
}

// Command Representation
enum Command {
    Get(String),
    Set(String, String),
}

// Start the server, managing connections
pub fn start_server<M>(mut manager: M) -> Result<(), Box<dyn Error>>
where
    M: Manager,
{
    let (mut recv, mut send) = manager.accept_new_connection().unwrap();

    // Shared spreadsheet with thread-safe access
    let spreadsheet = Arc::new(Mutex::new(Spreadsheet::new()));  

    // Setup Rhai engine outside the loop
    let mut engine = Engine::new();
    engine.register_fn("set_cell", move |spreadsheet: &mut Spreadsheet, cell_name: String, value: CellValue| {
        spreadsheet.set_cell_value(cell_name, value);
    });

    loop {
        info!("Just got message");
        let msg = recv.read_message()?;
        let reply = match parse_command(&msg) {
            Ok(Command::Get(cell_name)) => handle_get(&spreadsheet, &cell_name),
            Ok(Command::Set(cell_name, expr)) => handle_set(&spreadsheet, &mut engine, &cell_name, &expr),
            Err(err) => Reply::Error(err),
        };
        send.write_message(reply)?;
    }
}

// Command parsing using regular expressions
fn parse_command(msg: &str) -> Result<Command, String> {
    let regex = Regex::new(r"^(get|set)\s+(\w+)\s*(.*)$").unwrap(); 
    if let Some(captures) = regex.captures(msg) {
        let command_type = &captures[1];
        let cell_name = &captures[2];

        if validate_cell_name(cell_name) {
            match command_type {
                "get" => Ok(Command::Get(cell_name.to_string())),
                "set" => Ok(Command::Set(cell_name.to_string(), captures[3].to_string())),
                _ => Err("Invalid command type".to_string()) 
            }
        } else {
            Err("Invalid cell name".to_string())
        }
    } else {
        Err("Invalid command format".to_string())
    }
}

// Handle 'get' commands
fn handle_get(spreadsheet: &Arc<Mutex<Spreadsheet>>, cell_name: &str) -> Reply {
    let spreadsheet_data = spreadsheet.lock().unwrap();
    match spreadsheet_data.get_cell_value(cell_name) {
        Some(value) => Reply::Value(value.clone()),
        None => Reply::Value(CellValue::None),
    }
}

// Handle 'set' commands
fn handle_set(spreadsheet: &Arc<Mutex<Spreadsheet>>, engine: &mut Engine, cell_name: &str, expr: &str) -> Reply {
    let spreadsheet_data = spreadsheet.lock().unwrap();
    let mut variable_map = HashMap::new();
    let variable_names = CommandRunner::find_variables(expr);

    // Populate variable_map (you may need to modify this based on your variable handling)
    for var_name in variable_names {
       if let Some(value) = spreadsheet_data.get_cell_value(var_name) {
            variable_map.insert(var_name.to_string(), value.clone()); 
       }
    }

    drop(spreadsheet_data); // Release the mutex lock

    // Evaluate expression using Rhai
    let command_runner = CommandRunner::new(engine); 
    match command_runner.run_with_vars(expr, &variable_map) {
        Ok(value) => {
            let mut spreadsheet_data = spreadsheet.lock().unwrap();
            spreadsheet_data.set_cell_value(cell_name.to_string(), value.clone());
            Reply::Value(value)
        }
        Err(err) => Reply::Error(format!("Expression error: {}", err)),
    }
}

// Basic cell name validation 
fn validate_cell_name(cell_name: &str) -> bool {
    let regex = Regex::new(r"^[A-Z]+[1-9]\d*$").unwrap(); 
    regex.is_match(cell_name)
}