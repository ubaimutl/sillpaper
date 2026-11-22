#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Number,
    Comment,
    Type,
}

impl TokenKind {
    pub fn tag_name(self) -> &'static str {
        match self {
            TokenKind::Keyword => "code_kw",
            TokenKind::String => "code_str",
            TokenKind::Number => "code_num",
            TokenKind::Comment => "code_cmt",
            TokenKind::Type => "code_type",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LangFamily {
    CStyle,
    Python,
    Shell,
    Sql,
    Markup,
    Json,
    Generic,
}

fn classify_lang(lang: Option<&str>) -> LangFamily {
    let Some(raw) = lang else {
        return LangFamily::Generic;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "rs" | "rust" | "js" | "javascript" | "jsx" | "ts" | "typescript" | "tsx" | "c" | "h"
        | "cpp" | "cc" | "cxx" | "hpp" | "c++" | "go" | "golang" | "java" | "cs" | "csharp"
        | "c#" | "swift" | "kt" | "kotlin" | "zig" | "css" | "scss" | "php" | "dart" => {
            LangFamily::CStyle
        }
        "py" | "python" | "python3" => LangFamily::Python,
        "sh" | "bash" | "zsh" | "fish" | "shell" | "yaml" | "yml" | "toml" | "ini" | "conf"
        | "rb" | "ruby" | "dockerfile" | "makefile" | "make" => LangFamily::Shell,
        "sql" | "postgres" | "postgresql" | "sqlite" | "mysql" => LangFamily::Sql,
        "html" | "htm" | "xml" | "svg" => LangFamily::Markup,
        "json" | "jsonc" => LangFamily::Json,
        _ => LangFamily::Generic,
    }
}

pub fn highlight_code(text: &str, language: Option<&str>) -> Vec<(usize, usize, TokenKind)> {
    if text.is_empty() {
        return Vec::new();
    }
    let family = classify_lang(language);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut spans = Vec::new();
    let mut i = 0;

    let allows_slash_comments = matches!(
        family,
        LangFamily::CStyle | LangFamily::Json | LangFamily::Generic
    );
    let allows_hash_comments = matches!(
        family,
        LangFamily::Python | LangFamily::Shell | LangFamily::Generic
    );
    let allows_dash_comments = matches!(family, LangFamily::Sql);

    while i < n {
        let ch = chars[i];

        if is_line_break(ch) || ch.is_whitespace() {
            i += 1;
            continue;
        }

        if family == LangFamily::Markup && ch == '<' {
            if i + 3 < n && chars[i + 1] == '!' && chars[i + 2] == '-' && chars[i + 3] == '-' {
                let start = i;
                i += 4;
                while i + 2 < n && !(chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>')
                {
                    i += 1;
                }
                i = (i + 3).min(n);
                spans.push((start, i, TokenKind::Comment));
                continue;
            }
            let mut j = i + 1;
            if j < n && (chars[j] == '/' || chars[j] == '!' || chars[j] == '?') {
                j += 1;
            }
            let tag_start = j;
            while j < n && (chars[j].is_ascii_alphanumeric() || matches!(chars[j], '-' | '_' | ':'))
            {
                j += 1;
            }
            if j > tag_start {
                spans.push((tag_start, j, TokenKind::Keyword));
                i = j;
                continue;
            }
            i += 1;
            continue;
        }

        if allows_slash_comments && ch == '/' && i + 1 < n && chars[i + 1] == '/' {
            let start = i;
            i += 2;
            while i < n && !is_line_break(chars[i]) {
                i += 1;
            }
            spans.push((start, i, TokenKind::Comment));
            continue;
        }

        if allows_slash_comments && ch == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            spans.push((start, i, TokenKind::Comment));
            continue;
        }

        if allows_hash_comments
            && ch == '#'
            && !(family == LangFamily::Generic && i + 1 < n && matches!(chars[i + 1], '[' | '!'))
        {
            let start = i;
            i += 1;
            while i < n && !is_line_break(chars[i]) {
                i += 1;
            }
            spans.push((start, i, TokenKind::Comment));
            continue;
        }

        if allows_dash_comments && ch == '-' && i + 1 < n && chars[i + 1] == '-' {
            let start = i;
            i += 2;
            while i < n && !is_line_break(chars[i]) {
                i += 1;
            }
            spans.push((start, i, TokenKind::Comment));
            continue;
        }

        if ch == '"' || ch == '`' || (ch == '\'' && !is_rust_lifetime(&chars, i)) {
            let start = i;
            let quote = ch;
            if (quote == '"' || quote == '\'')
                && i + 2 < n
                && chars[i + 1] == quote
                && chars[i + 2] == quote
            {
                i += 3;
                while i + 2 < n
                    && !(chars[i] == quote && chars[i + 1] == quote && chars[i + 2] == quote)
                {
                    if chars[i] == '\\' && i + 1 < n {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                i = (i + 3).min(n);
                spans.push((start, i, TokenKind::String));
                continue;
            }

            i += 1;
            let multiline = quote == '`';
            while i < n {
                if chars[i] == '\\' && i + 1 < n {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                if !multiline && is_line_break(chars[i]) {
                    break;
                }
                i += 1;
            }
            spans.push((start, i, TokenKind::String));
            continue;
        }

        if ch == '@' && i + 1 < n && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') {
            let start = i;
            i += 1;
            while i < n && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '.')) {
                i += 1;
            }
            spans.push((start, i, TokenKind::Type));
            continue;
        }

        if ch.is_ascii_digit()
            && (i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_'))
        {
            let start = i;
            i += 1;
            while i < n
                && (chars[i].is_ascii_alphanumeric()
                    || chars[i] == '_'
                    || (chars[i] == '.' && i + 1 < n && chars[i + 1].is_ascii_digit()))
            {
                i += 1;
            }
            spans.push((start, i, TokenKind::Number));
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if is_literal_constant(&word, family) {
                spans.push((start, i, TokenKind::Number));
            } else if is_keyword(&word, family) {
                spans.push((start, i, TokenKind::Keyword));
            } else if is_builtin_type(&word, family)
                || (family == LangFamily::CStyle && i < n && chars[i] == '!')
                || is_pascal_case_type(&word)
            {
                let end = if family == LangFamily::CStyle && i < n && chars[i] == '!' {
                    i += 1;
                    i
                } else {
                    i
                };
                spans.push((start, end, TokenKind::Type));
            }
            continue;
        }

        i += 1;
    }

    spans
}

fn is_line_break(ch: char) -> bool {
    ch == '\n' || ch == '\r' || ch == '\u{2028}'
}

fn is_rust_lifetime(chars: &[char], idx: usize) -> bool {
    if idx + 1 >= chars.len() {
        return false;
    }
    let next = chars[idx + 1];
    if !(next.is_ascii_alphabetic() || next == '_') {
        return false;
    }
    if idx + 2 < chars.len() && chars[idx + 2] == '\'' {
        return false;
    }
    let mut j = idx + 1;
    while j < chars.len() && !is_line_break(chars[j]) {
        if chars[j] == '\'' {
            return false;
        }
        if !(chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
            return true;
        }
        j += 1;
    }
    true
}

fn is_pascal_case_type(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase() && chars.any(|c| c.is_ascii_lowercase())
}

fn is_literal_constant(word: &str, family: LangFamily) -> bool {
    match family {
        LangFamily::Sql => matches!(
            word.to_ascii_uppercase().as_str(),
            "TRUE" | "FALSE" | "NULL"
        ),
        _ => matches!(
            word,
            "true" | "false" | "null" | "nil" | "None" | "True" | "False" | "undefined" | "NaN"
        ),
    }
}

fn is_builtin_type(word: &str, family: LangFamily) -> bool {
    match family {
        LangFamily::CStyle | LangFamily::Generic => matches!(
            word,
            "bool"
                | "char"
                | "str"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "f32"
                | "f64"
                | "int"
                | "float"
                | "double"
                | "void"
                | "string"
                | "number"
                | "any"
                | "unknown"
                | "never"
                | "Self"
        ),
        LangFamily::Python => matches!(
            word,
            "int" | "float" | "str" | "bool" | "list" | "dict" | "set" | "tuple" | "bytes"
        ),
        _ => false,
    }
}

fn is_keyword(word: &str, family: LangFamily) -> bool {
    match family {
        LangFamily::Json | LangFamily::Markup => false,
        LangFamily::Sql => matches!(
            word.to_ascii_uppercase().as_str(),
            "SELECT"
                | "FROM"
                | "WHERE"
                | "JOIN"
                | "LEFT"
                | "RIGHT"
                | "INNER"
                | "OUTER"
                | "ON"
                | "AS"
                | "AND"
                | "OR"
                | "NOT"
                | "IN"
                | "IS"
                | "INSERT"
                | "INTO"
                | "VALUES"
                | "UPDATE"
                | "SET"
                | "DELETE"
                | "CREATE"
                | "TABLE"
                | "DROP"
                | "ALTER"
                | "ORDER"
                | "GROUP"
                | "BY"
                | "LIMIT"
                | "OFFSET"
                | "HAVING"
                | "DISTINCT"
                | "UNION"
                | "CASE"
                | "WHEN"
                | "THEN"
                | "ELSE"
                | "END"
        ),
        LangFamily::Python => matches!(
            word,
            "def"
                | "class"
                | "return"
                | "if"
                | "elif"
                | "else"
                | "for"
                | "while"
                | "in"
                | "not"
                | "and"
                | "or"
                | "import"
                | "from"
                | "as"
                | "with"
                | "try"
                | "except"
                | "finally"
                | "raise"
                | "pass"
                | "break"
                | "continue"
                | "lambda"
                | "yield"
                | "async"
                | "await"
                | "match"
                | "case"
                | "del"
                | "global"
                | "nonlocal"
                | "assert"
                | "is"
        ),
        LangFamily::Shell => matches!(
            word,
            "if" | "then"
                | "else"
                | "elif"
                | "fi"
                | "for"
                | "in"
                | "do"
                | "done"
                | "while"
                | "until"
                | "case"
                | "esac"
                | "function"
                | "local"
                | "export"
                | "return"
                | "exit"
                | "echo"
                | "set"
                | "unset"
                | "readonly"
                | "shift"
                | "source"
        ),
        LangFamily::CStyle | LangFamily::Generic => matches!(
            word,
            "fn" | "let"
                | "mut"
                | "const"
                | "static"
                | "pub"
                | "use"
                | "mod"
                | "crate"
                | "super"
                | "self"
                | "struct"
                | "enum"
                | "trait"
                | "impl"
                | "type"
                | "where"
                | "match"
                | "if"
                | "else"
                | "for"
                | "in"
                | "while"
                | "loop"
                | "break"
                | "continue"
                | "return"
                | "async"
                | "await"
                | "move"
                | "ref"
                | "unsafe"
                | "extern"
                | "as"
                | "function"
                | "var"
                | "class"
                | "extends"
                | "new"
                | "this"
                | "import"
                | "export"
                | "default"
                | "from"
                | "try"
                | "catch"
                | "finally"
                | "throw"
                | "switch"
                | "case"
                | "interface"
                | "implements"
                | "package"
                | "func"
                | "defer"
                | "go"
                | "chan"
                | "select"
                | "range"
                | "def"
                | "elif"
                | "pass"
                | "with"
                | "lambda"
                | "yield"
                | "public"
                | "private"
                | "protected"
                | "final"
                | "virtual"
                | "override"
                | "namespace"
                | "template"
                | "typename"
                | "sizeof"
                | "auto"
                | "union"
                | "typedef"
                | "include"
        ),
    }
}
