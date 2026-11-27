use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    stream::{BoxStream, StreamExt},
};
use iocraft::prelude::*;
use std::{
    io,
    time::{Duration, Instant},
};

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;
const SCROLL_STEPS: usize = 65;

#[derive(Clone)]
enum Kind {
    User,
    Assistant,
    Tool,
}

#[derive(Clone)]
struct Message {
    id: usize,
    kind: Kind,
    body: String,
}

fn conversation(count: usize) -> Vec<Message> {
    let words = [
        "the",
        "render",
        "loop",
        "scroll",
        "view",
        "layout",
        "terminal",
        "message",
        "conversation",
        "agent",
        "tool",
        "result",
        "streaming",
        "component",
        "update",
        "width",
        "height",
    ];
    (0..count)
        .map(|id| {
            let word_count = 8 + (id * 37) % 140;
            let mut body: String = (0..word_count)
                .map(|w| words[(id * 13 + w * 7) % words.len()])
                .collect::<Vec<_>>()
                .join(" ");
            if id % 5 == 0 {
                body.push_str(
                    "\n\nfn example() {\n    let x = compute();\n    println!(\"{x}\");\n}",
                );
            }
            let kind = match id % 4 {
                0 => Kind::User,
                1 => Kind::Assistant,
                _ => Kind::Tool,
            };
            Message { id, kind, body }
        })
        .collect()
}

#[derive(Default, Props)]
struct ConversationProps {
    messages: Vec<Message>,
}

#[component]
fn Conversation(props: &ConversationProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut scrolls = hooks.use_state(|| 0usize);
    hooks.use_terminal_events(move |event| {
        if let TerminalEvent::FullscreenMouse(_) = event {
            scrolls += 1;
        }
    });
    if scrolls.get() >= SCROLL_STEPS {
        system.exit();
    }

    element! {
        View(width: WIDTH, height: HEIGHT, flex_direction: FlexDirection::Column) {
            Text(content: "header")
            View(width: 100pct, height: 0, flex_grow: 1.0, padding_left: 1, padding_right: 1, padding_top: 1, margin_bottom: 1) {
                ScrollView(auto_scroll: true, keyboard_scroll: Some(false)) {
                    View(width: 100pct, flex_direction: FlexDirection::Column, row_gap: 1) {
                        #(props.messages.iter().map(message))
                    }
                }
            }
            View(height: 3, border_style: BorderStyle::Single) {
                Text(content: "prompt")
            }
            Text(content: "footer")
        }
    }
}

fn message(m: &Message) -> AnyElement<'static> {
    match m.kind {
        Kind::User | Kind::Assistant => element! {
            View(key: m.id, flex_direction: FlexDirection::Column) {
                Text(content: if matches!(m.kind, Kind::User) { "you" } else { "assistant" }, weight: Weight::Bold)
                Text(content: m.body.clone(), wrap: TextWrap::Wrap)
            }
        }
        .into_any(),
        Kind::Tool => element! {
            View(key: m.id, flex_direction: FlexDirection::Column, padding_left: 1) {
                View(flex_direction: FlexDirection::Row) {
                    Text(content: "✓")
                    Text(content: "read_file", weight: Weight::Bold)
                    Text(content: " · done")
                    View(flex_grow: 1.0)
                    Button {
                        Text(content: "[details]")
                    }
                }
                View(padding_left: 2) {
                    Text(content: m.body.chars().take(80).collect::<String>())
                }
            }
        }
        .into_any(),
    }
}

#[derive(Default)]
struct Timing {
    first_frame: Option<Instant>,
    last_frame: Option<Instant>,
    canvas_writes: usize,
}

struct BenchBackend<'a> {
    tx: UnboundedSender<io::Result<TerminalEvent>>,
    rx: Option<UnboundedReceiver<io::Result<TerminalEvent>>>,
    timing: &'a mut Timing,
}

impl TerminalBackend for BenchBackend<'_> {
    fn size(&self) -> Option<(u16, u16)> {
        Some((WIDTH, HEIGHT))
    }

    fn clear_canvas(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_canvas(&mut self, _prev: Option<&Canvas>, _canvas: &Canvas) -> io::Result<()> {
        self.timing.canvas_writes += 1;
        Ok(())
    }

    fn print_above(&mut self, _messages: &[Passthrough<'_>]) -> io::Result<()> {
        Ok(())
    }

    // One scroll step per frame: the render loop drains every ready event before rendering, so
    // releasing them all at once would collapse the run into a single frame.
    fn end_frame(&mut self) -> io::Result<()> {
        let now = Instant::now();
        self.timing.first_frame.get_or_insert(now);
        self.timing.last_frame = Some(now);
        self.tx
            .unbounded_send(Ok(TerminalEvent::FullscreenMouse(
                FullscreenMouseEvent::new(MouseEventKind::ScrollUp, 10, 10),
            )))
            .expect("the render loop should still be receiving scroll events");
        Ok(())
    }

    fn event_stream(&mut self) -> io::Result<BoxStream<'static, io::Result<TerminalEvent>>> {
        Ok(self.rx.take().expect("event stream requested once").boxed())
    }
}

fn measure_scroll(messages: &[Message]) -> Duration {
    let (tx, rx) = unbounded();
    let mut timing = Timing::default();
    let backend = BenchBackend {
        tx,
        rx: Some(rx),
        timing: &mut timing,
    };
    let mut element = element!(Conversation(messages: messages.to_vec()));
    smol::block_on(element.render_loop().fullscreen().backend(backend))
        .expect("the scroll sequence should finish");
    assert!(
        timing.canvas_writes > 1,
        "scrolling should change the canvas"
    );
    timing
        .last_frame
        .expect("the render loop should draw a frame")
        - timing
            .first_frame
            .expect("the render loop should draw a frame")
}

fn scroll(c: &mut Criterion) {
    let mut group = c.benchmark_group("scroll");
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Elements(SCROLL_STEPS as u64));

    for message_count in [50, 150, 400] {
        let messages = conversation(message_count);
        group.bench_with_input(
            BenchmarkId::from_parameter(message_count),
            &messages,
            |b, messages| {
                b.iter_custom(|iterations| (0..iterations).map(|_| measure_scroll(messages)).sum());
            },
        );
    }
    group.finish();
}

criterion_group!(benches, scroll);
criterion_main!(benches);
