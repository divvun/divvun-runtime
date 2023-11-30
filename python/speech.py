from typing import Union
from . import Arg, Entry, Command

def tts(input: Union[Command, Entry], voice_model_path: str, hifigan_model_path: str) -> Command:
    return Command(
        cmd="speech::tts",
        args={
            "voice_model_path": Arg(type="string", value=voice_model_path),
            "hifigan_model_path": Arg(type="string", value=hifigan_model_path),
        },
        input=input
    )