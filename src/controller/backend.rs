use crate::state::{
    backend::{Backend, Sqlite},
    paths, AppState, Session, SpentTime,
};
use druid::{
    widget::Controller, Env, Event, EventCtx, ExtEventSink, LifeCycle, LifeCycleCtx, Target, Widget,
};
use std::{
    error::Error,
    sync::mpsc::{self, Sender},
    thread,
};

enum BackendCommand {
    AddAction(String),
    AddSubject(String),
    AddSession(Session, SpentTime),
    Stop,
}

#[derive(Eq, PartialEq)]
enum Continue {
    Yes,
    No,
}

pub mod msg {
    use crate::state::{Action, Session, Subject};
    use druid::Selector;

    pub const STOP: Selector = Selector::new("zeitig.backend.stop");

    pub const ADD_ACTION: Selector<String> = Selector::new("zeitig.backend.add-action");
    pub const ADD_SUBJECT: Selector<String> = Selector::new("zeitig.backend.add-subject");
    pub const ADD_SESSION: Selector<Session> = Selector::new("zeitig.backend.add-session");

    pub const ACTION_ADDED: Selector<Action> = Selector::new("zeitig.backend.action-added");
    pub const SUBJECT_ADDED: Selector<Subject> = Selector::new("zeitig.backend.subject-added");

    pub const STOPPED: Selector = Selector::new("zeitig.backend.stopped");
    pub const ERROR: Selector<String> = Selector::new("zeitig.backend.error");
}

#[derive(Default)]
pub struct BackendController {
    sender: Option<Sender<BackendCommand>>,
}

impl BackendController {
    pub fn new() -> BackendController {
        Self::default()
    }

    fn init(&mut self, ctx: &mut LifeCycleCtx) {
        assert!(self.sender.is_none());

        let (sender, receiver) = mpsc::channel();
        let sink = ctx.get_external_handle();
        let mut backend = Sqlite::new(paths::data_file()).unwrap();
        backend.setup().unwrap();
        thread::spawn(move || {
            loop {
                let cmd = receiver.recv().expect(
                    "The backend channel should not be closed while the backend is running.",
                );
                match Self::handle_command(cmd, &mut backend, &sink) {
                    Ok(Continue::Yes) => {}
                    Ok(Continue::No) => break,
                    Err(err) => {
                        log::error!("{}", err);
                        let message = err.to_string();
                        if sink
                            .submit_command(msg::ERROR, message, Target::Auto)
                            .is_err()
                        {
                            log::error!(
                                "Backend event sink has been closed while the backend is still running."
                            );
                            break;
                        }
                    }
                }
            }
            if sink.submit_command(msg::STOPPED, (), Target::Auto).is_err() {
                log::info!(
                    "Backend event sink has been closed before emitting the stopped command."
                );
            }
        });

        self.sender = Some(sender);
    }

    fn handle_command(
        cmd: BackendCommand,
        backend: &mut dyn Backend,
        sink: &ExtEventSink,
    ) -> Result<Continue, Box<dyn Error>> {
        match cmd {
            BackendCommand::AddAction(name) => {
                let action = backend.create_action(&name)?;
                sink.submit_command(msg::ACTION_ADDED, action, Target::Auto)?;
            }
            BackendCommand::AddSubject(name) => {
                let subject = backend.create_subject(&name)?;
                sink.submit_command(msg::SUBJECT_ADDED, subject, Target::Auto)?;
            }
            BackendCommand::AddSession(session, total_duration) => {
                backend.add_session(&session)?;
                backend.update_time(&session.topic, &total_duration)?;
            }
            BackendCommand::Stop => return Ok(Continue::No),
        }
        Ok(Continue::Yes)
    }
}

impl<W: Widget<AppState>> Controller<AppState, W> for BackendController {
    fn event(
        &mut self,
        child: &mut W,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut AppState,
        env: &Env,
    ) {
        let sender = self.sender.as_ref().unwrap();
        match event {
            Event::Command(cmd) if cmd.is(msg::ADD_ACTION) => {
                let name = cmd.get_unchecked(msg::ADD_ACTION).to_owned();
                sender.send(BackendCommand::AddAction(name)).unwrap();
            }
            Event::Command(cmd) if cmd.is(msg::ADD_SUBJECT) => {
                let name = cmd.get_unchecked(msg::ADD_SUBJECT).to_owned();
                sender.send(BackendCommand::AddSubject(name)).unwrap();
            }
            Event::Command(cmd) if cmd.is(msg::ADD_SESSION) => {
                let session = cmd.get_unchecked(msg::ADD_SESSION).to_owned();
                let past_duration = data.content.time_table.get(&session.topic);
                let total_duration = past_duration + session.duration();
                sender
                    .send(BackendCommand::AddSession(session, total_duration))
                    .unwrap();
            }
            Event::Command(cmd) if cmd.is(msg::STOP) => {
                sender.send(BackendCommand::Stop).unwrap();
            }
            _ => child.event(ctx, event, data, env),
        }
    }

    fn lifecycle(
        &mut self,
        child: &mut W,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &AppState,
        env: &Env,
    ) {
        if let LifeCycle::WidgetAdded = event {
            self.init(ctx);
        }
        child.lifecycle(ctx, event, data, env)
    }
}
