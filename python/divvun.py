from typing import Union
from . import Arg, Entry, Command

def blanktag(model_path: str, input: Union[Command, Entry]) -> Command:
    return Command(
        cmd="divvun::blanktag",
        args={
            "model_path": Arg(type="path", value=model_path),
        },
        input=input
    )

def cgspell(err_model_path: str, acc_model_path: str, input: Union[Command, Entry]) -> Command:
    return Command(
        cmd="divvun::cgspell",
        args={
            "err_model_path": Arg(type="path", value=err_model_path),
            "acc_model_path": Arg(type="path", value=acc_model_path),
        },
        input=input
    )

def suggest(model_path: str, error_xml_path: str, input: Union[Command, Entry]) -> Command:
    return Command(
        cmd="divvun::suggest",
        args={
            "model_path": Arg(type="path", value=model_path),
            "error_xml_path": Arg(type="path", value=error_xml_path),
        },
        input=input
    )