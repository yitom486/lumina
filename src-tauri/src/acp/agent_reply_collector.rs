//! Keep only the last agent reply segment after tool calls (ACP may emit
//! preamble text before tools and the real answer after).

#[derive(Default)]
pub struct AgentReplyCollector {
    segments: Vec<String>,
    current: String,
}

impl AgentReplyCollector {
    pub fn push_agent_chunk(&mut self, text: &str) {
        self.current.push_str(text);
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
}
