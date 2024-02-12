from typing import Any, Dict, Optional, Union, Literal, Callable, Tuple, List
from contextvars import ContextVar

_current = ContextVar("current", default={})

ValueType = Literal["string", "path"]


class _Entry:
    def __init__(self, value_type: ValueType):
        self.type = "entry"
        self.value_type = value_type


class StringEntry(_Entry):
    def __init__(self):
        super().__init__("string")


class PathEntry(_Entry):
    def __init__(self):
        super().__init__("path")


class Arg:
    def __init__(self, type: str, value: Optional[str]):
        self.type = type
        self.value = value


_InputSingle = Union["Command", _Entry]
_InputMultiple = List[_InputSingle]

Input = Union[_InputSingle, _InputMultiple]


class Command:
    def __init__(
        self,
        module: str,
        command: str,
        input: Input,
        returns: str,
        args: Optional[Dict[str, Arg]] = None,
    ):
        self.type = "command"
        self.module = module
        self.command = command
        if args is not None:
            self.args = args
        self.input = input
        self.returns = returns

        _current.get()[hex(id(self))] = self


class _Ref:
    def __init__(self, something: Union["Command", _Entry]):
        self.type = "ref"
        if isinstance(something, Command):
            self.ref = hex(id(something))
        else:
            self.ref = "#/entry"

def pipeline(func: Callable[..., Any]) -> Callable[..., Any]:
    entry = func.__annotations__.get("entry", None)
    if entry is None:
        raise ValueError(f"Pipeline function missing `entry` argument")
    if not issubclass(entry, _Entry):
        raise ValueError(
            f"Pipeline function `entry` argument must be an Entry subclass"
        )

    def wrapper():
        _current.set({})
        e = entry()
        output = _Ref(func(e))
        commands = _current.get()
        _current.set({})
        for command in commands.values():
            if isinstance(command.input, list):
                command.input = [_Ref(x) for x in command.input]
            elif isinstance(command.input, (Command, _Entry)):
                command.input = _Ref(command.input)
            else:
                raise Exception(f"Unknown input type: {type(command.input)}")
        return (e, output, commands)

    setattr(wrapper, "_is_pipeline", True)
    return wrapper


def merge(*inputs: Tuple[Input], returns: str) -> Command:
    return Command(
        input=list(inputs),
        module="std",
        command="merge",
        returns=returns,
    )
    

def _to_json(entry: _Entry, output: _Ref, commands: Dict) -> str:
    import json
    from json import JSONEncoder

    class Encoder(JSONEncoder):
        def default(self, o: Any):
            return o.__dict__

    return json.dumps({
        "commands": commands,
        "entry": entry,
        "output": output
    }, cls=Encoder, indent=2)
