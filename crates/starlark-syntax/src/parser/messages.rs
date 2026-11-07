//! English parsing messages from `StarlarkBundle.properties`.

pub(crate) fn message(key: &str) -> String {
    match key {
        "parser.expected.expression" => "Expression expected".to_owned(),
        "parser.expected.string" => "String expected".to_owned(),
        "parser.expected.rbracket" => "']' expected".to_owned(),
        "parser.expected.expr.or.comma.or.bracket" => "Expected expression, ',' or ']'".to_owned(),
        "parser.expected.in" => "'in' expected".to_owned(),
        "parser.expected.for.or.bracket" => "']' or 'for' expected".to_owned(),
        "parser.expected.comma" => "',' expected".to_owned(),
        "parser.expected.colon" => "':' expected".to_owned(),
        "parser.expected.rpar" => "')' expected".to_owned(),
        "parser.expected.lpar" => "'(' expected".to_owned(),
        "parser.expected.rbrace" => "'}' expected".to_owned(),
        "parser.expected.name" => "Name expected".to_owned(),
        "parser.expected.colon.or.rbracket" => "':' or ']' expected".to_owned(),
        "parser.expected.comma.or.rpar" => "',' or ')' expected".to_owned(),
        "parser.expected.else" => "'else' expected".to_owned(),
        "parser.expected.identifier" => "Identifier expected".to_owned(),
        "parser.expected.comma.lpar.rpar" => "',' or '(' or ')' expected".to_owned(),
        "parser.expected.formal.param.name" => "Formal parameter name expected".to_owned(),
        "parser.reserved.keyword.cannot.be.used.as.identifier" => {
            "'{0}' reserved keyword can't be used as an identifier".to_owned()
        }
        "parser.statement.expected.found.0" => "Statement expected, found {0}".to_owned(),
        "parser.unexpected.indent" => "Unexpected indent".to_owned(),
        "parser.unindent.does.not.match.any.outer.indent" => {
            "Unindent does not match any outer indentation level".to_owned()
        }
        "parser.expected.statement" => "Statement expected".to_owned(),
        "parser.indent.expected" => "Indent expected".to_owned(),
        "parser.end.of.statement.expected" => "End of statement expected".to_owned(),
        "parser.rbracket.or.comma.expected" => "']' or ',' expected".to_owned(),
        "parser.unexpected.expression.syntax" => "Unexpected expression syntax".to_owned(),
        "parser.tuple.expression.expected" => "Tuple expression expected".to_owned(),
        "parser.value.expression.expected" => "Value expression expected".to_owned(),
        _ => panic!("unknown Starlark parser message key: {key}"),
    }
}

pub(crate) fn message_with(key: &str, arguments: &[&str]) -> String {
    let mut message = message(key);
    for (index, argument) in arguments.iter().enumerate() {
        message = message.replace(&format!("{{{index}}}"), argument);
    }
    message
}
