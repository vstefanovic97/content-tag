use crate::{Options, Preprocessor as CorePreprocessor};
use std::{
    fmt,
    str,
    path::PathBuf,
};
use swc_common::{
    errors::Handler,
    sync::{Lock, Lrc},
    SourceMap, Spanned,
};
use swc_error_reporters::{GraphicalReportHandler, GraphicalTheme, PrettyEmitter};
use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};
use serde_wasm_bindgen::from_value;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = Error)]
    fn js_error(message: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = JSON, js_name = parse)]
    fn json_parse(value: JsValue) -> JsValue;
}

#[wasm_bindgen]
pub struct Preprocessor {
    // TODO: reusing this between calls result in incorrect spans; there may
    // be value in reusing some part of the stack but we will have to figure
    // out how to combine the APIs correctly to ensure we are not hanging on
    // to the states unexpectedly
    // core: Box<CorePreprocessor>,
}

#[derive(Serialize, Deserialize)]
pub struct ProcessOptions {
    filename: Option<String>,
    inline_source_map: bool,
}

#[derive(Clone, Default)]
struct Writer(Lrc<Lock<String>>);

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.lock().write_str(s)
    }
}

fn capture_err_detail(
    err: swc_ecma_parser::error::Error,
    source_map: Lrc<SourceMap>,
    theme: GraphicalTheme,
) -> JsValue {
    let wr = Writer::default();
    let emitter = PrettyEmitter::new(
        source_map,
        Box::new(wr.clone()),
        GraphicalReportHandler::new_themed(theme),
        Default::default(),
    );
    let handler = Handler::with_emitter(true, false, Box::new(emitter));
    err.into_diagnostic(&handler).emit();
    let s = wr.0.lock().as_str().to_string();
    s.into()
}

fn as_javascript_error(err: swc_ecma_parser::error::Error, source_map: Lrc<SourceMap>) -> JsValue {
    let short_desc = format!("Parse Error at {}", source_map.span_to_string(err.span()));
    let js_err = js_error(short_desc.into());
    js_sys::Reflect::set(
        &js_err,
        &"source_code".into(),
        &capture_err_detail(
            err.clone(),
            source_map.clone(),
            GraphicalTheme::unicode_nocolor(),
        ),
    )
    .unwrap();
    js_sys::Reflect::set(
        &js_err,
        &"source_code_color".into(),
        &capture_err_detail(err, source_map, GraphicalTheme::unicode()),
    )
    .unwrap();
    return js_err;
}

#[wasm_bindgen]
impl Preprocessor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        // TODO: investigate reuse
        // Self {
        //     core: Box::new(CorePreprocessor::new()),
        // }
        Self {}
    }

    pub fn process(&self, src: String, options: JsValue) -> Result<String, JsValue> {
        let options: ProcessOptions = from_value(options)
            .map_err(|e| js_error(format!("Options parsing error: {:?}", e).into()))?;

        let preprocessor = CorePreprocessor::new();
        let result = preprocessor.process(
            &src,
            Options {
                filename: options.filename.map(|f| PathBuf::from(f)),
                inline_source_map: options.inline_source_map,
            },
        );

        match result {
            Ok(output) => Ok(output),
            Err(err) => Err(as_javascript_error(err, preprocessor.source_map()).into()),
        }
    }

    pub fn parse(&self, src: String, options: JsValue) -> Result<JsValue, JsValue> {
        let parse_options: ProcessOptions = from_value(options.clone())
            .map_err(|e| js_error(format!("Options parsing error: {:?}", e).into()))?;

        let preprocessor = CorePreprocessor::new();
        let result = preprocessor
            .parse(
                &src,
                Options {
                    filename: parse_options.filename.map(|f| PathBuf::from(f)),
                    inline_source_map: parse_options.inline_source_map,
                },
            )
            .map_err(|_err| self.process(src, options).unwrap_err())?;
        let serialized = serde_json::to_string(&result)
            .map_err(|err| js_error(format!("Unexpected serialization error; please open an issue with the following debug info: {err:#?}").into()))?;
        Ok(json_parse(serialized.into()))
    }
}
