from . import hfst, cg3, divvun, Entry, to_json

def generate_pipeline():
    x = hfst.tokenize("./tokeniser-gramcheck-gt-desc.pmhfst", Entry(value_type="string"))
    x = divvun.blanktag("./analyser-gt-whitespace.hfst", x)
    x = cg3.vislcg3("./valency.bin", x)
    x = cg3.vislcg3("./mwe-dis.bin", x)
    x = cg3.mwesplit(x)
    x = divvun.cgspell("./errmodel.default.hfst", "./acceptor.default.hfst", x)
    x = cg3.vislcg3("./valency-postspell.bin", x)
    x = cg3.vislcg3("./grc-disambiguator.bin", x)
    x = cg3.vislcg3("./grammarchecker.bin", x)
    x = cg3.vislcg3("./grammarchecker-release.bin", x)
    x = divvun.suggest("./generator-gramcheck-gt-norm.hfstol", "./errors.xml", x)
    
    print(to_json(x, indent=2))