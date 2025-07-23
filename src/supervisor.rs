use crate::{
    actor::{Exit, Pid, Signal},
    async_actor::{IntoAsyncActor, SimpleActor, into_actor},
    global,
};

type Factory = Box<dyn Fn() -> Pid + Send + 'static>;

#[derive(Copy, Clone, PartialEq, Eq)]
enum ChildState {
    Running,
    Stopping,
    Restarting,
    Stopped,
    Dead,
}

pub enum RestartPolicy {
    Permanent,
    Transient,
    Temporary,
}

struct Child {
    pid: Pid,
    factory: Factory,
    policy: RestartPolicy,
    state: ChildState,
}

impl Child {
    fn should_restart(&self, reason: &Exit) -> bool {
        match self.policy {
            RestartPolicy::Permanent => true,
            RestartPolicy::Transient => !matches!(reason, Exit::Normal | Exit::Shutdown),
            RestartPolicy::Temporary => false,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum SupervisorState {
    Idle,
    Stopping(usize),
}

#[derive(Copy, Clone)]
pub enum Strategy {
    /// One child is restarted if it fails.
    OneForOne,
    /// All children are restarted if one fails.
    OneForAll,
    /// All the children after the failing one are restarted if one fails.
    RestForOne,
}

impl Strategy {
    fn is_affected(self, index: usize, failed_index: usize) -> bool {
        match self {
            Strategy::OneForOne => index == failed_index,
            Strategy::OneForAll => true,
            Strategy::RestForOne => index >= failed_index,
        }
    }
}

struct SupervisorActor {
    children: Vec<Child>,
    strategy: Strategy,
    state: SupervisorState,
}

enum Request {
    Supervise(Factory, RestartPolicy),
}

impl SupervisorActor {
    pub fn new(strategy: Strategy) -> Self {
        Self {
            children: Vec::new(),
            strategy,
            state: SupervisorState::Idle,
        }
    }

    fn failed_index(&self, pid: Pid) -> Option<usize> {
        self.children.iter().position(|child| child.pid == pid)
    }
}

impl SimpleActor for SupervisorActor {
    type Message = Request;

    async fn handle(&mut self, message: Self::Message) -> Option<Exit> {
        match message {
            Request::Supervise(factory, policy) => {
                let pid = factory();
                self.children.push(Child {
                    pid,
                    factory,
                    policy,
                    state: ChildState::Running,
                });
            }
        }

        None
    }

    async fn started(&mut self) -> Option<Exit> {
        global::trap_exit(true);

        None
    }

    async fn on_exit(&mut self, from: Pid, reason: Exit) -> Option<Exit> {
        let is_child = self.children.iter().any(|c| c.pid == from);
        if !is_child && reason != Exit::Normal {
            return Some(reason);
        } else if from == global::pid() {
            return Some(reason);
        }

        match (self.children.len(), self.strategy) {
            (_, Strategy::OneForOne) | (1, Strategy::RestForOne) | (1, Strategy::OneForAll) => {
                let child = self.children.iter_mut().find(|child| child.pid == from)?;

                if child.should_restart(&reason) {
                    let pid = (child.factory)();
                    child.pid = pid;
                }
            }
            (_, Strategy::RestForOne) | (_, Strategy::OneForAll) => {
                if self.state == SupervisorState::Idle {
                    let failed_index = self.failed_index(from)?;
                    let mut affected = 0;

                    for (index, child) in self.children.iter_mut().enumerate() {
                        if child.state == ChildState::Running
                            && self.strategy.is_affected(index, failed_index)
                        {
                            affected += 1;

                            if child.should_restart(&reason) {
                                child.state = ChildState::Restarting;
                            } else {
                                child.state = ChildState::Stopping;
                            }

                            global::send_signal(child.pid, Signal::Kill);
                        }
                    }

                    self.state = SupervisorState::Stopping(affected);
                } else if let SupervisorState::Stopping(affected) = &mut self.state {
                    let child = self.children.iter_mut().find(|child| child.pid == from)?;

                    if child.state == ChildState::Restarting {
                        child.state = ChildState::Stopped;
                    } else {
                        child.state = ChildState::Dead;
                    }

                    *affected -= 1;

                    if *affected == 0 {
                        for child in self.children.iter_mut() {
                            if child.state == ChildState::Stopped {
                                if child.should_restart(&reason) {
                                    let pid = (child.factory)();
                                    child.pid = pid;
                                    child.state = ChildState::Running;
                                }
                            }
                        }

                        self.state = SupervisorState::Idle;
                    }
                }
            }
        }

        None
    }
}

pub struct Supervisor {
    actor: Pid,
}

impl Supervisor {
    pub const fn invalid() -> Self {
        Self {
            actor: Pid::invalid(),
        }
    }

    pub fn spawn_linked(strategy: Strategy) -> Self {
        let actor = SupervisorActor::new(strategy);
        let actor_ref = global::spawn_linked(into_actor(actor));
        Self { actor: actor_ref }
    }

    pub fn supervise<F, B>(&self, policy: RestartPolicy, factory: F)
    where
        B: IntoAsyncActor,
        F: Fn() -> B + Send + 'static,
    {
        assert!(self.actor != Pid::invalid(), "Supervisor is invalid");

        let factory = Box::new(move || {
            // Do spawn
            let actor = factory();
            let actor = global::spawn_linked(actor);
            actor
        });

        crate::global::send(self.actor, Request::Supervise(factory, policy));
    }

    pub fn supervise_named<F, B>(&self, name: &'static str, policy: RestartPolicy, factory: F)
    where
        B: IntoAsyncActor,
        F: Fn() -> B + Send + 'static,
    {
        assert!(self.actor != Pid::invalid(), "Supervisor is invalid");

        let factory = Box::new(move || {
            // Do spawn
            let actor = factory();
            let actor = global::spawn_linked(actor);
            global::register(name, actor);
            actor
        });

        crate::global::send(self.actor, Request::Supervise(factory, policy));
    }
}
