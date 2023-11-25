from typing import Union
from . import Arg, Entry, Command

def mwesplit(input: Union[Command, Entry]) -> Command:
    return Command(
        cmd="cg3::mwesplit",
        input=input
    )

def vislcg3(model_path: str, input: Union[Command, Entry]) -> Command:
    return Command(
        cmd="cg3::vislcg3",
        args={
            "model_path": Arg(type="path", value=model_path),
        },
        input=input
    )