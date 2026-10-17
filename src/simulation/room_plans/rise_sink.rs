use super::{RoomPlanner, SwitchContext, SwitchPlan};

/// Every wall that goes sinks and every new one rises, in place.
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
    use super::super::WallChange;
    use super::*;

    #[test]
    fn walls_rise_and_sink_even_when_they_line_up() {
        let plan = plan_with(&RiseAndSink, &[(0, 0)], &[(1, 0)]);
        assert_eq!(
            plan.changes,
            vec![
                WallChange::Sink {
                    at: cells(&[(0, 0)])[0]
                },
                WallChange::Rise {
                    at: cells(&[(1, 0)])[0]
                },
            ]
        );
    }
}
