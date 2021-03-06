extern crate piston_window;
extern crate rusty_v8 as v8;

use piston_window::Context as PistonContext;
use piston_window::G2d;
use piston_window::PistonWindow;
use piston_window::WindowSettings;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::env;
use std::fs;
use v8::Array;
use v8::Context;
use v8::ContextScope;
use v8::Exception;
use v8::Function;
use v8::FunctionCallback;
use v8::FunctionCallbackArguments;
use v8::FunctionTemplate;
use v8::HandleScope;
use v8::Isolate;
use v8::Local;
use v8::MapFnTo;
use v8::Number;
use v8::Object;
use v8::ObjectTemplate;
use v8::ReturnValue;
use v8::Script;
use v8::TryCatch;
use v8::Value;
use v8::V8;

struct RenderState(PistonContext, &'static mut G2d<'static>);

thread_local!(static RENDER_STATE: RefCell<Option<RenderState>> = RefCell::new(None));

fn main() {
    let args: Vec<String> = env::args().collect();

    let flags = args.iter().skip(1).take_while(|arg| arg.starts_with("--"));
    let has_arg = |name| flags.clone().any(|arg| *arg == name);

    let help = has_arg("--help");
    let predictable = has_arg("--predictable");

    let args = V8::set_flags_from_command_line(args);

    if help {
        return;
    }

    if predictable {
        V8::set_entropy_source(|buf| {
            for c in buf {
                *c = 42;
            }
            true
        });
    }

    let filename = &args[1];
    let args: Vec<&String> = args.iter().skip(2).collect();
    let source = fs::read_to_string(filename).expect("can't read source file");

    V8::initialize_platform(v8::new_default_platform().unwrap());
    V8::initialize();
    let isolate = &mut Isolate::new(Default::default());
    let scope = &mut HandleScope::new(isolate);
    let global_object_template = make_global_object_template(scope);
    let console_object_template = make_console_object_template(scope);

    let context = Context::new_from_template(scope, global_object_template);
    let scope = &mut ContextScope::new(scope, context);

    let args_string = v8::String::new(scope, "args").unwrap();
    let args_array = Array::new(scope, args.len().try_into().unwrap());
    for (index, arg) in args.iter().enumerate() {
        let arg = v8::String::new(scope, arg).unwrap();
        let index = v8::Integer::new(scope, index.try_into().unwrap());
        args_array.set(scope, index.into(), arg.into());
    }

    // Setting the "console" property through the global template doesn't work...
    let console_string = v8::String::new(scope, "console").unwrap();
    let console_object = console_object_template.new_instance(scope).unwrap();

    let global = context.global(scope);
    global.set(scope, args_string.into(), args_array.into());
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

    let keydown_string = v8::String::new(scope, "keydown").unwrap();
    let keyup_string = v8::String::new(scope, "keyup").unwrap();
    let idle_string = v8::String::new(scope, "idle").unwrap();
    let mousedown_string = v8::String::new(scope, "mousedown").unwrap();
    let mouseup_string = v8::String::new(scope, "mouseup").unwrap();
    let render_string = v8::String::new(scope, "render").unwrap();
    let update_string = v8::String::new(scope, "update").unwrap();

    while let Some(event) = window.next() {
        use piston_window::*;

        match &event {
            Event::Input(Input::Button(args), _) => match args.button {
                Button::Keyboard(key) => {
                    let s = format!("{:?}", key); // Maybe not a good idea...
                    let s = v8::String::new(scope, &s).unwrap();
                    let name = match args.state {
                        ButtonState::Press => keydown_string,
                        ButtonState::Release => keyup_string,
                    };
                    call_method(scope, global, name, &[s.into()]);
                }
                Button::Mouse(button) => {
                    let s = format!("{:?}", button); // Maybe not a good idea...
                    let s = v8::String::new(scope, &s).unwrap();
                    let name = match args.state {
                        ButtonState::Press => mousedown_string,
                        ButtonState::Release => mouseup_string,
                    };
                    call_method(scope, global, name, &[s.into()]);
                }
                _ => {}
            },
            Event::Loop(Loop::Idle(args)) => {
                let dt = Number::new(scope, args.dt);
                call_method(scope, global, idle_string, &[dt.into()]);
            }
            Event::Loop(Loop::Update(args)) => {
                let dt = Number::new(scope, args.dt);
                call_method(scope, global, update_string, &[dt.into()]);
            }
            _ => {}
        };

        window.draw_2d(&event, |context, graphics, _| {
            RENDER_STATE.with(|slot| {
                let state = RenderState(context, unsafe {
                    std::mem::transmute(graphics)
                });
                slot.replace(Some(state));
                call_method(scope, global, render_string, &[]);
                slot.replace(None);
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

fn call_method<'s>(
    scope: &mut HandleScope<'s>,
    object: Local<Object>,
    name: Local<v8::String>,
    args: &[Local<Value>],
) {
    if let Some(fun) = object.get(scope, name.into()) {
        if let Ok(fun) = Local::<Function>::try_from(fun) {
            let scope = &mut HandleScope::new(scope);
            let scope = &mut TryCatch::new(scope);
            if fun.call(scope, object.into(), args).is_none() {
                print_try_catch(scope);
            }
        }
    }
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

fn print_try_catch(try_catch: &mut TryCatch<HandleScope>) {
    let exc = try_catch.exception().unwrap();
    let exc = exc.to_string(try_catch).unwrap();
    eprintln!("{}", exc.to_rust_string_lossy(try_catch));
    // TODO(bnoordhuis) print stack trace
}

fn with_render_state<F>(scope: &mut HandleScope, f: F)
where
    F: FnOnce(PistonContext, &mut G2d),
{
    RENDER_STATE.with(|slot| {
        if let Some(RenderState(ctx, gfx)) = &mut *slot.borrow_mut() {
            f(*ctx, gfx);
        } else {
            let msg = v8::String::new(scope, "not rendering").unwrap();
            let exc = Exception::error(scope, msg);
            scope.throw_exception(exc);
        }
    });
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
    let color = [r, g, b, a];
    with_render_state(scope, |_, gfx| piston_window::clear(color, gfx));
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
    with_render_state(scope, |ctx, gfx| {
        piston_window::rectangle(color, coords, ctx.transform, gfx)
    });
}
