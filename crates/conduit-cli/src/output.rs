#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    pub(crate) fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

pub(crate) fn fields(items: impl IntoIterator<Item = (&'static str, &'static str)>) -> String {
    items
        .into_iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn json_object(items: impl IntoIterator<Item = (&'static str, &'static str)>) -> String {
    let entries = items
        .into_iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", escape(key), escape(value)))
        .collect::<Vec<_>>()
        .join(",");

    format!("{{{entries}}}")
}

fn escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            ch => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_compact_fields() {
        assert_eq!(
            fields([("name", "conduit"), ("version", "0.1.0")]),
            "name: conduit\nversion: 0.1.0"
        );
    }

    #[test]
    fn renders_compact_json() {
        assert_eq!(
            json_object([("message", "quoted \"value\"")]),
            "{\"message\":\"quoted \\\"value\\\"\"}"
        );
    }
}
