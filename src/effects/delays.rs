use crate::common::params::CcNode;
use fundsp::audionode::FrameId;
use fundsp::combinator::An;
use fundsp::feedback::Feedback;
use fundsp::prelude64::{Net, U1, Unit, feedback, tap, unit};

fn cc_controlled_delay(
    feedback_cc: Net,
    audio_net: Net,
    delay_time: CcNode,
) -> An<Feedback<U1, Unit<U1, U1>, FrameId<U1>>> {
    let delay_line = (audio_net | delay_time) >> tap(0.01, 5.0) * feedback_cc;
    let db = unit::<U1, U1>(Box::new(delay_line));
    feedback(db)
}
