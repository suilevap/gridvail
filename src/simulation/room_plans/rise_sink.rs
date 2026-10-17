use super::{RoomPlanner, SwitchContext, SwitchPlan};

/// Every wall that goes sinks and every new one rises, in place: slides of
/// no length.
#[derive(Clone, Copy, Debug, Default)]
pub struct RiseAndSink;

impl RoomPlanner for RiseAndSink {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan {
        SwitchPlan::rise_and_sink(switch)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cells, plan_with};
    use super::super::WallMove;
    use super::*;

    #[test]
    fn walls_rise_and_sink_even_when_they_line_up() {
        let plan = plan_with(&RiseAndSink, &[(0, 0)], &[(1, 0)]);
        assert_eq!(
            plan.moves,
            vec![
                WallMove::sink_at(cells(&[(0, 0)])[0]),
                WallMove::rise_at(cells(&[(1, 0)])[0]),
            ]
        );
    }
}
