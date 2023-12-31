// Copyright (c) 2023 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions,
// more details in file LICENSE, LICENSE.additional and CONTRIBUTING.

use std::{any::Any, fmt::Display};

use anna_types::CompileError;

pub mod ast;
pub mod lexer;
pub mod native_assembly_instruction;
pub mod parser;
pub mod peekable_iterator;

pub const NAME_PATH_SEPARATOR: &str = "::";

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error: {}", self.message)
    }
}

impl CompileError for ParseError {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
