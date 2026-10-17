"""Checks the analyzed Bazel dependency graph, including package contents."""

_DependencyGraph = provider(fields = ["labels"])

def _collect(target, ctx):
    transitive = []
    if ctx.rule:
        for attribute in ["deps", "srcs"]:
            for dependency in getattr(ctx.rule.attr, attribute, []):
                if _DependencyGraph in dependency:
                    transitive.append(dependency[_DependencyGraph].labels)
    return [_DependencyGraph(labels = depset([str(target.label)], transitive = transitive))]

_graph = aspect(implementation = _collect, attr_aspects = ["deps", "srcs"])

def _check(ctx):
    labels = ctx.attr.subject[_DependencyGraph].labels.to_list()
    for forbidden in ctx.attr.forbidden:
        matches = [label for label in labels if forbidden in label]
        if matches:
            fail("{} crosses dependency boundary {}: {}".format(ctx.attr.subject.label, forbidden, matches))
    for required in ctx.attr.required:
        if not any([required in label for label in labels]):
            fail("{} is missing required dependency {}".format(ctx.attr.subject.label, required))
    executable = ctx.actions.declare_file(ctx.label.name + ".sh")
    ctx.actions.write(executable, "#!/bin/sh\nexit 0\n", is_executable = True)
    return [DefaultInfo(executable = executable)]

dependency_boundary_test = rule(
    implementation = _check,
    test = True,
    attrs = {
        "subject": attr.label(mandatory = True, aspects = [_graph]),
        "forbidden": attr.string_list(),
        "required": attr.string_list(),
    },
)
