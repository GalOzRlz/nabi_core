use crate::common::params::CcAudioNode;
use fundsp::audionode::FrameId;
use fundsp::combinator::An;
use fundsp::feedback::Feedback;
use fundsp::prelude64::{Net, U1, Unit, feedback, tap, unit};

fn cc_controlled_delay(
    min_delay: f32,
    max_delay: f32,
    feedback_cc: Net,
    audio_net: Net,
    delay_time: CcAudioNode,
) -> An<Feedback<U1, Unit<U1, U1>, FrameId<U1>>> {
    let delay_line = (audio_net | delay_time) >> tap(min_delay, max_delay) * feedback_cc;
    let db = unit::<U1, U1>(Box::new(delay_line));
    feedback(db)
}
