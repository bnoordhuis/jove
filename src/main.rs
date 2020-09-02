extern crate piston_window;
extern crate rusty_v8 as v8;

use piston_window::Context as PistonContext;
use piston_window::G2d;
use piston_window::PistonWindow;
use piston_window::WindowSettings;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::env;
use std::fs;
use v8::Context;
use v8::ContextScope;
use v8::Function;
use v8::FunctionCallback;
use v8::FunctionCallbackArguments;
use v8::FunctionTemplate;
use v8::HandleScope;
use v8::Isolate;
use v8::Local;
use v8::MapFnTo;
use v8::ObjectTemplate;
use v8::ReturnValue;
use v8::Script;
use v8::TryCatch;
use v8::Value;
use v8::V8;

type Command = Box<dyn FnOnce(PistonContext, &mut G2d)>;

thread_local!(static COMMANDS: RefCell<Vec<Command>> = RefCell::new(vec![]));

fn main() {
    let args: Vec<String> = env::args().collect();
    let filename = &args[1];
    let source = fs::read_to_string(filename).expect("can't read source file");

    V8::initialize_platform(v8::new_default_platform().unwrap());
    V8::initialize();
    let isolate = &mut Isolate::new(Default::default());
    let scope = &mut HandleScope::new(isolate);
    let global_object_template = make_global_object_template(scope);
    let console_object_template = make_console_object_template(scope);

    let context = Context::new_from_template(scope, global_object_template);
    let scope = &mut ContextScope::new(scope, context);

    // Setting the "console" property through the global template doesn't work...
    let console_string = v8::String::new(scope, "console").unwrap();
    let console_object = console_object_template.new_instance(scope).unwrap();
    let global = context.global(scope);
    global.set(scope, console_string.into(), console_object.into());

    let scope = &mut TryCatch::new(scope);
    let result = eval_in_context(scope, &source);

    if let Some(result) = result {
        eprintln!("{}", to_string_or(scope, result, "Uncaught exception"));
    } else if let Some(exception) = scope.exception() {
        eprintln!("{}", to_string_or(scope, exception, "Uncaught exception"));
    }

    let mut window: PistonWindow = WindowSettings::new("jove", [800, 600])
        .exit_on_esc(true)
        .build()
        .unwrap();

    let draw_string = v8::String::new(scope, "draw").unwrap();

    while let Some(event) = window.next() {
        if let Some(draw) = global.get(scope, draw_string.into()) {
            if let Ok(draw) = Local::<Function>::try_from(draw) {
                let scope = &mut HandleScope::new(scope);
                if draw.call(scope, global.into(), &[]).is_none() {
                    // TODO(bnoordhuis) Log exceptions.
                }
            }
        }

        window.draw_2d(&event, |context, graphics, _| {
            COMMANDS.with(|commands| {
                let commands = commands.replace(vec![]);
                for command in commands {
                    command(context, graphics);
                }
            });
        });
    }
}

fn eval_in_context<'s>(
    scope: &mut HandleScope<'s>,
    source: &str,
) -> Option<Local<'s, Value>> {
    let source = v8::String::new(scope, source).unwrap();
    let script = Script::compile(scope, source, None).unwrap();
    script.run(scope)
}

fn make_global_object_template<'s>(
    scope: &mut HandleScope<'s, ()>,
) -> Local<'s, ObjectTemplate> {
    let global_object_template = ObjectTemplate::new(scope);
    add_method(scope, global_object_template, "clear", clear_callback);
    add_method(
        scope,
        global_object_template,
        "rectangle",
        rectangle_callback,
    );
    global_object_template
}

fn make_console_object_template<'s>(
    scope: &mut HandleScope<'s, ()>,
) -> Local<'s, ObjectTemplate> {
    let console_object_template = ObjectTemplate::new(scope);
    add_method(scope, console_object_template, "log", console_log_callback);
    console_object_template
}

fn add_method<'s>(
    scope: &mut HandleScope<'s, ()>,
    template: Local<ObjectTemplate>,
    name: &str,
    callback: impl MapFnTo<FunctionCallback>,
) {
    let function_template = FunctionTemplate::new(scope, callback);
    let name_string = v8::String::new(scope, name).unwrap();
    template.set(name_string.into(), function_template.into());
}

fn to_string<'s>(
    scope: &mut HandleScope<'s>,
    value: Local<Value>,
) -> Option<String> {
    match value.to_string(scope) {
        Some(string) => Some(string.to_rust_string_lossy(scope)),
        None => None,
    }
}

fn to_string_or<'s>(
    scope: &mut HandleScope<'s>,
    value: Local<Value>,
    default: &str,
) -> String {
    match to_string(scope, value) {
        Some(string) => string,
        None => default.to_string(),
    }
}

fn console_log_callback(
    scope: &mut HandleScope,
    args: FunctionCallbackArguments,
    _: ReturnValue,
) {
    for i in 0..args.length() {
        let arg = to_string_or(scope, args.get(i), "<exception>");
        print!("{}", arg);
    }
    println!();
}

fn clear_callback(
    scope: &mut HandleScope,
    args: FunctionCallbackArguments,
    _: ReturnValue,
) {
    let r = args.get(0).number_value(scope).unwrap_or(0f64) as f32;
    let g = args.get(1).number_value(scope).unwrap_or(0f64) as f32;
    let b = args.get(2).number_value(scope).unwrap_or(0f64) as f32;
    let a = args.get(3).number_value(scope).unwrap_or(0f64) as f32;
    let color: [f32; 4] = [r, g, b, a];
    COMMANDS.with(|commands| {
        commands.borrow_mut().push(Box::new(move |_, gfx| {
            piston_window::clear(color, gfx);
        }));
    });
}

#[allow(clippy::many_single_char_names)]
fn rectangle_callback(
    scope: &mut HandleScope,
    args: FunctionCallbackArguments,
    _: ReturnValue,
) {
    let r = args.get(0).number_value(scope).unwrap_or(0f64) as f32;
    let g = args.get(1).number_value(scope).unwrap_or(0f64) as f32;
    let b = args.get(2).number_value(scope).unwrap_or(0f64) as f32;
    let a = args.get(3).number_value(scope).unwrap_or(0f64) as f32;
    let x = args.get(4).number_value(scope).unwrap_or(0f64);
    let y = args.get(5).number_value(scope).unwrap_or(0f64);
    let w = args.get(6).number_value(scope).unwrap_or(0f64);
    let h = args.get(7).number_value(scope).unwrap_or(0f64);
    let color: [f32; 4] = [r, g, b, a];
    let coords: [f64; 4] = [x, y, w, h];
    COMMANDS.with(|commands| {
        commands.borrow_mut().push(Box::new(move |ctx, gfx| {
            piston_window::rectangle(color, coords, ctx.transform, gfx);
        }));
    });
}
