from json import JSONEncoder
import json
from typing import Any, Dict, Optional, Union

class Entry:
    def __init__(self, value_type: str):
        self.type = "entry"
        self.value_type = value_type

class Arg:
    def __init__(self, type: str, value: Optional[str]):
        self.type = type
        self.value = value

class Command:
    def __init__(self, cmd: str, args: Optional[Dict[str, Arg]] = None, input: Optional[Union["Command", Entry]] = None):
        self.type = "command"
        self.cmd = cmd
        self.args = args
        self.input = input

def to_json(obj: Command, indent: Optional[int] = None) -> str:
    class Encoder(JSONEncoder):
        def default(self, o: Any):
            return o.__dict__
    return json.dumps(obj, cls=Encoder, indent=indent)
