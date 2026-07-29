use orquestra_core::config::OutputFormat;
use serde::Serialize;

pub trait OutputData: Serialize {
    fn render_human(&self) -> String;
    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{ \"error\": \"{e}\" }}"))
    }
    fn render_jsonl(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| format!("{{ \"error\": \"{e}\" }}"))
    }
}

pub fn print_output(data: &impl OutputData, format: &OutputFormat) {
    match format {
        OutputFormat::Human => println!("{}", data.render_human()),
        OutputFormat::Json => println!("{}", data.render_json()),
        OutputFormat::Jsonl => println!("{}", data.render_jsonl()),
    }
}
