use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;
use tokio::task::JoinHandle;

use crate::ast;

use super::{
    CommandRunner, Context, Error, PipelineEvent, PipelineValue, PipelineValueRx, PipelineValueTx,
    PipelineValues, Tap,
};

/// Streaming test command: trickles `count` values out one at a time, with
/// `delay_ms` between each. The body of each emitted value is the input
/// string suffixed with `"#N"` where N is the index.
///
/// Exists so cancellation can be tested end-to-end before any real streaming
/// command lands (e.g. the upcoming streaming `speech::tts`). Doubles as the
/// reference implementation pattern for `forward_stream` overrides: the inner
/// `tokio::select!` races a sleep against `input_rx.recv()` so a Cancel event
/// arriving mid-emission halts the current input's stream and lets the
/// command keep listening for the next input.
#[derive(facet::Facet)]
pub struct Trickle {
    pub count: u32,
    pub delay_ms: u64,
}

#[rt_command(
    module = "debug",
    name = "trickle",
    input = [String],
    output = "String",
    args = [count = "Int", delay_ms = "Int"]
)]
impl Trickle {
    pub async fn new(
        _context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        let count = kwargs
            .get("count")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as u32)
            .ok_or_else(|| Error::msg("Missing count").at("pipeline.json", "/args/count"))?;
        let delay_ms = kwargs
            .get("delay_ms")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as u64)
            .ok_or_else(|| Error::msg("Missing delay_ms").at("pipeline.json", "/args/delay_ms"))?;
        Ok(Arc::new(Self { count, delay_ms }))
    }
}

#[async_trait]
impl CommandRunner for Trickle {
    async fn forward(
        self: Arc<Self>,
        _input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, Error> {
        // forward() is unused — we override forward_stream so we can emit
        // values over time and react to mid-stream Cancel events.
        Err(Error::msg("debug::trickle uses forward_stream"))
    }

    fn forward_stream(
        self: Arc<Self>,
        mut input_rx: PipelineValueRx,
        output: PipelineValueTx,
        _tap: Option<Tap>,
        _config: Arc<serde_json::Value>,
    ) -> JoinHandle<Result<(), Error>> {
        let name = self.name().to_string();
        let count = self.count;
        let delay = Duration::from_millis(self.delay_ms);

        tokio::spawn(async move {
            tracing::debug!("{name}: forward_stream task started");
            loop {
                let event = input_rx.recv().await.map_err(Error::wrap)?;
                match event {
                    PipelineEvent::Value(value) => {
                        let s = value.try_into_string()?;
                        let mut cancelled = false;
                        for i in 0..count {
                            tokio::select! {
                                biased;
                                ev = input_rx.recv() => match ev.map_err(Error::wrap)? {
                                    PipelineEvent::Cancel => {
                                        tracing::debug!("{name}: Cancel mid-emission at i={i}");
                                        output.send(PipelineEvent::Cancel).map_err(Error::wrap)?;
                                        cancelled = true;
                                        break;
                                    }
                                    PipelineEvent::Close => {
                                        tracing::debug!("{name}: Close mid-emission at i={i}");
                                        output.send(PipelineEvent::Close).map_err(Error::wrap)?;
                                        return Ok(());
                                    }
                                    PipelineEvent::Error(e) => {
                                        output.send(PipelineEvent::Error(e.clone())).map_err(Error::wrap)?;
                                        return Err(e);
                                    }
                                    other => {
                                        // Finish / Value arriving mid-stream is unusual; just pass through.
                                        output.send(other).map_err(Error::wrap)?;
                                    }
                                },
                                _ = tokio::time::sleep(delay) => {
                                    let v: PipelineValue = format!("{s}#{i}").into();
                                    output.send(PipelineEvent::Value(v)).map_err(Error::wrap)?;
                                }
                            }
                        }
                        if !cancelled {
                            output.send(PipelineEvent::Finish).map_err(Error::wrap)?;
                        }
                    }
                    PipelineEvent::Cancel => {
                        output.send(PipelineEvent::Cancel).map_err(Error::wrap)?;
                    }
                    PipelineEvent::Finish => {
                        output.send(PipelineEvent::Finish).map_err(Error::wrap)?;
                    }
                    PipelineEvent::Error(e) => {
                        output
                            .send(PipelineEvent::Error(e.clone()))
                            .map_err(Error::wrap)?;
                        return Err(e);
                    }
                    PipelineEvent::Close => {
                        output.send(PipelineEvent::Close).map_err(Error::wrap)?;
                        break;
                    }
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "debug::trickle"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::broadcast;

    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_stops_emission_keeps_command_alive() {
        let trickle = Arc::new(Trickle {
            count: 100,
            delay_ms: 10,
        });
        let (in_tx, in_rx) = broadcast::channel(16);
        let (out_tx, mut out_rx) = broadcast::channel(64);
        let handle = trickle
            .clone()
            .forward_stream(in_rx, out_tx, None, Arc::new(json!({})));

        // 1. Send Input "a"; collect a few Value events.
        in_tx
            .send(PipelineEvent::Value(PipelineValue::String("a".into())))
            .expect("send input");

        let mut received: Vec<String> = Vec::new();
        while received.len() < 3 {
            match tokio::time::timeout(Duration::from_secs(2), out_rx.recv())
                .await
                .expect("timed out waiting for value")
                .expect("recv")
            {
                PipelineEvent::Value(PipelineValue::String(s)) => received.push(s),
                _ => continue,
            }
        }
        assert_eq!(
            received,
            vec!["a#0".to_string(), "a#1".to_string(), "a#2".to_string()]
        );

        // 2. Send Cancel; assert a Cancel echo arrives and no more "a#N" values
        //    follow after that.
        in_tx.send(PipelineEvent::Cancel).expect("send cancel");

        let mut saw_cancel = false;
        let mut surprise_value: Option<String> = None;
        loop {
            match tokio::time::timeout(Duration::from_millis(500), out_rx.recv()).await {
                Ok(Ok(PipelineEvent::Cancel)) => {
                    saw_cancel = true;
                    break;
                }
                Ok(Ok(PipelineEvent::Value(PipelineValue::String(s)))) if s.starts_with("a#") => {
                    // Tolerate a couple of in-flight emissions that crossed
                    // the wire before Cancel was processed. The strict check
                    // is "no more Values after the Cancel echo".
                    received.push(s);
                }
                Ok(Ok(PipelineEvent::Value(PipelineValue::String(s)))) => {
                    surprise_value = Some(s);
                    break;
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) | Err(_) => break,
            }
        }
        assert!(saw_cancel, "expected Cancel echo, got: {surprise_value:?}");

        // No more emissions from "a" should arrive after Cancel.
        let res = tokio::time::timeout(Duration::from_millis(80), out_rx.recv()).await;
        match res {
            Err(_) => {} // timeout — good, nothing more came
            Ok(Ok(PipelineEvent::Value(PipelineValue::String(s)))) if s.starts_with("a#") => {
                panic!("got '{s}' after Cancel echo — emission did not actually stop");
            }
            Ok(_) => {} // non-Value or recv error — fine
        }

        // 3. Send a fresh Input "b"; assert emission resumes for the new value.
        in_tx
            .send(PipelineEvent::Value(PipelineValue::String("b".into())))
            .expect("send second input");

        let mut got_b = false;
        for _ in 0..20 {
            if let Ok(Ok(PipelineEvent::Value(PipelineValue::String(s)))) =
                tokio::time::timeout(Duration::from_millis(200), out_rx.recv()).await
            {
                if s.starts_with("b#") {
                    got_b = true;
                    break;
                }
            }
        }
        assert!(
            got_b,
            "pipeline should be alive and emit for the next input"
        );

        // 4. Send Close; assert the task exits cleanly.
        in_tx.send(PipelineEvent::Close).expect("send close");
        let join = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(join.is_ok(), "task did not exit after Close within timeout");
    }
}
