use directories::ProjectDirs;
use druid::{
    widget::{
        Button, CrossAxisAlignment, Either, Flex, Label, List, MainAxisAlignment, Painter,
        SizedBox, TextBox,
    },
    AppLauncher, Command, Data, EventCtx, Selector, UnitPoint, Widget, WidgetExt, WindowDesc,
};
use match_macro::match_widget;
use std::{
    mem,
    path::{Path, PathBuf},
    time::Duration,
};

mod state;
use state::{Action, ActiveSession, AppState, Creating, DateTime, Session, SpentTime, Subject};

mod enter;
use enter::EnterController;

mod auto_saver;
use auto_saver::{AutoSaver, SAVE_NOW};

mod command_receiver;
use command_receiver::CommandReceiver;

mod ticker;
use ticker::Ticker;

const SELECT_ACTION: Selector = Selector::new("zeitig.select_action");
const SELECT_SUBJECT: Selector = Selector::new("zeitig.select_subject");

fn read_state() -> AppState {
    let path = data_file_path();
    if path.exists() {
        let data = std::fs::read(path).expect("Failed to read data.");
        rmp_serde::from_slice(&data).expect("Failed to deserialize data.")
    } else {
        AppState::default()
    }
}

fn write_state(state: AppState) {
    let path = data_file_path();
    let data = rmp_serde::to_vec(&state).expect("Failed to serialize data.");
    std::fs::write(path, &data).expect("Failed to write data.");
}

fn data_file_path() -> PathBuf {
    if cfg!(debug_symbols) {
        return PathBuf::from("./zeitig.mp");
    }
    if let Some(pd) = ProjectDirs::from("de", "leopoldluley", "Zeitig") {
        let data = pd.data_dir();
        if std::fs::create_dir_all(data).is_ok() {
            return data.join("zeitig.mp");
        }
    }
    Path::new("zeitig.mp").to_owned()
}

fn main() {
    let window = WindowDesc::new(ui)
        .title("Zeitig")
        .window_size((300.0, 400.0));

    let state = read_state();

    AppLauncher::with_window(window)
        .launch(state)
        .expect("Failed to launch Zeitig.");
}

fn start_new_session(data: &mut AppState) {
    data.active = Some(ActiveSession {
        started: DateTime::now(),
        duration: SpentTime::default(),
    })
}

fn end_session(data: &mut AppState) {
    if data.active.is_some() {
        let active = data.active.take().unwrap();
        if *active.duration > Duration::from_secs(30) {
            let session = Session {
                action: data.selected_action.clone().unwrap(),
                subject: data.selected_subject.clone().unwrap(),
                started: active.started,
                duration: active.duration,
                ended: DateTime::now(),
            };
            data.history.push_back(session);
        }
    }
}

fn selected_action_label() -> impl Widget<Option<Action>> {
    match_widget! { Option<Action>,
        Some(Action) => Label::dynamic(|action: &Action, _| format!("{}", action.as_ref())),
        None => Label::new("No Action"),
    }
}

fn selected_subject_label() -> impl Widget<Option<Subject>> {
    match_widget! { Option<Subject>,
        Some(Subject) => Label::dynamic(|subject: &Subject, _| format!("{}", subject.as_ref())),
        None => Label::new("No Subject"),
    }
}

fn session_duration_label() -> impl Widget<Option<ActiveSession>> {
    match_widget! { Option<ActiveSession>,
        Some(ActiveSession) => Label::dynamic(|session: &ActiveSession, _| format!("Session: {}", session.duration)),
        None => Label::new("Session: not running"),
    }
}

fn separator<T: Data>() -> impl Widget<T> {
    use druid::RenderContext;
    Painter::new(|ctx, _, env| {
        let bounds = ctx.size().to_rect();
        ctx.fill(bounds, &env.get(druid::theme::BORDER_DARK));
    })
    .expand_width()
    .fix_height(2.0)
}

fn ui() -> impl Widget<AppState> {
    Flex::column()
        .with_child(
            Flex::column()
                .with_child(
                    Flex::row()
                        .with_flex_child(
                            Flex::column()
                                .cross_axis_alignment(CrossAxisAlignment::Start)
                                .with_child(session_duration_label().lens(AppState::active))
                                .with_child(
                                    Label::dynamic(|time, _| format!("Total: {}", time))
                                        .lens(AppState::spent_time),
                                )
                                .expand_width(),
                            1.0,
                        )
                        .with_spacer(5.0)
                        .with_child(
                            Button::new(|data: &AppState, _: &_| match data.active {
                                None => "Start".to_string(),
                                Some(_) => "Stop".to_string(),
                            })
                            .on_click(|_, data: &mut AppState, _| {
                                match data.active {
                                    Some(_) => end_session(data),
                                    None => start_new_session(data),
                                }
                            }),
                        )
                        .padding((10.0, 10.0, 10.0, 5.0))
                        .controller(Ticker::new()),
                )
                .with_child(
                    Flex::row()
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .with_child(
                            selected_action_label()
                                .lens(AppState::selected_action)
                                .align_horizontal(UnitPoint::CENTER),
                        )
                        .with_spacer(5.0)
                        .with_child(
                            selected_subject_label()
                                .lens(AppState::selected_subject)
                                .align_horizontal(UnitPoint::CENTER),
                        )
                        .controller(CommandReceiver::new(|_, data: &mut AppState, cmd| {
                            if cmd.selector == SELECT_ACTION {
                                end_session(data);
                                let action = cmd.get_object::<Action>().unwrap();
                                data.selected_action = Some(action.clone());
                            }
                            if cmd.selector == SELECT_SUBJECT {
                                end_session(data);
                                let subject = cmd.get_object::<Subject>().unwrap();
                                data.selected_subject = Some(subject.clone());
                            }
                        })),
                ),
        )
        .with_spacer(5.0)
        .with_child(separator())
        .with_spacer(10.0)
        .with_flex_child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .with_flex_child(
                    List::new(|| {
                        Label::dynamic(|action: &Action, _| action.as_ref().to_string())
                            .padding(3.0)
                            .on_click(|ctx, action, _| {
                                ctx.submit_command(
                                    Command::new(SELECT_ACTION, action.clone()),
                                    None,
                                );
                            })
                            .align_horizontal(UnitPoint::CENTER)
                    })
                    .lens(AppState::actions)
                    .expand_width(),
                    1.0,
                )
                .with_flex_child(
                    List::new(|| {
                        Label::dynamic(|subject: &Subject, _| subject.as_ref().to_string())
                            .padding(3.0)
                            .on_click(|ctx, subject, _| {
                                ctx.submit_command(
                                    Command::new(SELECT_SUBJECT, subject.clone()),
                                    None,
                                );
                            })
                            .align_horizontal(UnitPoint::CENTER)
                    })
                    .lens(AppState::subjects)
                    .expand_width(),
                    1.0,
                )
                .expand_height(),
            1.0,
        )
        .with_child(Either::new(
            |data, _| data.creating != Creating::None,
            Flex::column()
                .with_child(
                    Label::dynamic(|data: &AppState, _| {
                        match data.creating {
                            Creating::None => "No Title",
                            Creating::Action => "Add new action",
                            Creating::Subject => "Add new subject",
                        }
                        .to_string()
                    })
                    .expand_width(),
                )
                .with_child(
                    TextBox::new()
                        .lens(AppState::creating_name)
                        .controller(EnterController::new(
                            |ctx: &mut EventCtx, data: &mut AppState| match data.creating {
                                Creating::None => (),
                                Creating::Action => {
                                    data.actions
                                        .push_back(Action::new(mem::take(&mut data.creating_name)));
                                    ctx.submit_command(SAVE_NOW, None);
                                }
                                Creating::Subject => {
                                    data.subjects.push_back(Subject::new(mem::take(
                                        &mut data.creating_name,
                                    )));
                                    ctx.submit_command(SAVE_NOW, None);
                                }
                            },
                        ))
                        .expand_width(),
                )
                .padding(5.0),
            SizedBox::empty(),
        ))
        .with_child(
            Flex::row()
                .with_flex_child(
                    Button::new(|data: &AppState, _: &_| {
                        if data.creating == Creating::Action {
                            "Cancel"
                        } else {
                            "New Action"
                        }
                        .into()
                    })
                    .on_click(|_, data: &mut AppState, _| {
                        data.creating = if data.creating == Creating::Action {
                            Creating::None
                        } else {
                            Creating::Action
                        }
                    })
                    .expand_width(),
                    1.0,
                )
                .with_flex_child(
                    Button::new(|data: &AppState, _: &_| {
                        if data.creating == Creating::Subject {
                            "Cancel"
                        } else {
                            "New Subject"
                        }
                        .into()
                    })
                    .on_click(|_, data: &mut AppState, _| {
                        data.creating = if data.creating == Creating::Subject {
                            Creating::None
                        } else {
                            Creating::Subject
                        }
                    })
                    .expand_width(),
                    1.0,
                ),
        )
        .controller(AutoSaver::new())
}
