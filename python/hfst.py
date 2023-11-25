from typing import Union
from . import Arg, Entry, Command

def tokenize(model_path: str, input: Union[Command, Entry]) -> Command:
    return Command(
        cmd="hfst::tokenize",
        args={
            "model_path": Arg(type="string", value=model_path),
        },
        input=input
    )