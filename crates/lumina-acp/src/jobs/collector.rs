//! Keep only the last agent reply segment after tool calls (ACP may emit
//! preamble text before tools and the real answer after).

#[derive(Default)]
pub struct AgentReplyCollector {
    segments: Vec<String>,
    current: String,
    chunks: u32,
}

impl AgentReplyCollector {
    pub fn push_agent_chunk(&mut self, text: &str) {
        self.chunks += 1;
        self.current.push_str(text);
    }

    /// How many `agent_message_chunk` updates arrived (empty ones included).
    /// P0b telemetry: distinguishes "silent session" from "no events at all".
    pub fn chunk_count(&self) -> u32 {
        self.chunks
    }

    pub fn on_tool_call(&mut self) {
        if !self.current.trim().is_empty() {
            self.segments.push(std::mem::take(&mut self.current));
        }
    }

    pub fn finish(mut self) -> String {
        if !self.current.trim().is_empty() {
            self.segments.push(self.current);
        }
        self.segments
            .into_iter()
            .rev()
            .find(|segment| !segment.trim().is_empty())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_last_agent_segment_after_tool_call() {
        let mut collector = AgentReplyCollector::default();
        collector.push_agent_chunk("Natural English: prelude\n");
        collector.on_tool_call();
        collector.push_agent_chunk("最终中文答案");
        assert_eq!(collector.finish(), "最终中文答案");
    }

    #[test]
    fn single_segment_without_tools() {
        let mut collector = AgentReplyCollector::default();
        collector.push_agent_chunk("只有一段回复");
        assert_eq!(collector.finish(), "只有一段回复");
    }

    #[test]
    fn chunk_count_tracks_updates_including_empty() {
        let mut collector = AgentReplyCollector::default();
        assert_eq!(collector.chunk_count(), 0);
        collector.push_agent_chunk("");
        collector.push_agent_chunk("hi");
        assert_eq!(collector.chunk_count(), 2);
        // Empty-only traffic still finishes empty (typed NoOutput upstream).
        let mut silent = AgentReplyCollector::default();
        silent.push_agent_chunk("   ");
        assert_eq!(silent.chunk_count(), 1);
        assert!(silent.finish().trim().is_empty());
    }
}
