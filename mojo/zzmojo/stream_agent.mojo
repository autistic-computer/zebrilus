from .agent_events import AgentPaneState
from .agent_pane_model import (
    AgentEffect,
    EFFECT_PROMPT,
    EFFECT_CANCEL,
    EFFECT_UNQUEUE,
    EFFECT_PERMISSION,
    EFFECT_ANSWER,
    EFFECT_SET_OPTION,
    EFFECT_REPLAY,
    EFFECT_ACKNOWLEDGE,
)
from .stream_events import (
    AgentWireState,
    agent_prompt,
    agent_cancel,
    agent_unqueue,
    agent_permission,
    agent_answer,
    agent_set_option,
    agent_replay,
    agent_acknowledge,
)


def agent_pane_state(state: AgentWireState) -> AgentPaneState:
    var result = AgentPaneState()
    result.phase = state.phase
    result.failure = state.failure.copy()
    result.queued_prompts = Int(state.queued_prompts)
    result.session_id = state.session_id.copy()
    result.title = state.title.copy()
    result.error = state.error.copy()
    result.config_options = state.config_options.copy()
    if state.permission:
        result.parked = True
        result.permission_id = state.permission.value().request_id
        result.permission_payload = state.permission.value().payload.copy()
    return result^


def agent_effect_frame(
    pane: UInt64, effect: AgentEffect
) raises -> List[UInt8]:
    if effect.kind == EFFECT_PROMPT:
        var formats = List[String]()
        var images = List[List[UInt8]]()
        for image in effect.images:
            formats.append(image.format.copy())
            images.append(image.data.copy())
        return agent_prompt(pane, effect.text, formats, images)
    if effect.kind == EFFECT_CANCEL:
        return agent_cancel(pane)
    if effect.kind == EFFECT_UNQUEUE:
        return agent_unqueue(pane)
    if effect.kind == EFFECT_PERMISSION:
        return agent_permission(pane, effect.request, effect.option)
    if effect.kind == EFFECT_ANSWER:
        var ids = List[String]()
        var answers = List[List[String]]()
        for answer in effect.answers:
            ids.append(answer.question.copy())
            answers.append(answer.answers.copy())
        return agent_answer(pane, effect.request, ids, answers)
    if effect.kind == EFFECT_SET_OPTION:
        if not effect.option:
            raise Error("agent option effect has no option id")
        return agent_set_option(pane, effect.option.value(), effect.text)
    if effect.kind == EFFECT_REPLAY:
        return agent_replay(pane, effect.request)
    if effect.kind == EFFECT_ACKNOWLEDGE:
        return agent_acknowledge(pane, effect.request)
    raise Error("unknown agent effect")
