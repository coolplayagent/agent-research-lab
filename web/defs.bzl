"""Compile modular TypeScript with the pinned Node/TypeScript toolchain."""

def _typescript_impl(ctx):
    output = ctx.actions.declare_directory(ctx.label.name)
    ctx.actions.run(
        executable = ctx.executable._node,
        arguments = [ctx.file._tsc.path, "--project", ctx.file.config.path, "--outDir", output.path],
        inputs = depset(ctx.files.srcs + [ctx.file.config] + ctx.files._compiler),
        outputs = [output],
        mnemonic = "TypeScript",
    )
    return [DefaultInfo(files = depset([output]))]

typescript = rule(
    implementation = _typescript_impl,
    attrs = {
        "srcs": attr.label_list(allow_files = [".ts"]),
        "config": attr.label(allow_single_file = [".json"]),
        "_node": attr.label(default = "@im_node//:bin/node", executable = True, allow_single_file = True, cfg = "exec"),
        "_tsc": attr.label(default = "@im_typescript//:bin/tsc", allow_single_file = True),
        "_compiler": attr.label(default = "@im_typescript//:compiler", allow_files = True),
    },
)

def _node_test_impl(ctx):
    runner = ctx.actions.declare_file(ctx.label.name + ".sh")
    ctx.actions.write(runner, "#!/bin/sh\nexec \"$TEST_SRCDIR/$TEST_WORKSPACE/" + ctx.executable._node.short_path + "\" \"$TEST_SRCDIR/$TEST_WORKSPACE/" + ctx.file.script.short_path + "\" \"$TEST_SRCDIR/$TEST_WORKSPACE/" + ctx.file._typescript.short_path + "\"\n", is_executable = True)
    files = ctx.files.data + ctx.files._compiler + [ctx.file.script, ctx.executable._node]
    return [DefaultInfo(executable = runner, runfiles = ctx.runfiles(files = files))]

node_test = rule(
    implementation = _node_test_impl,
    test = True,
    attrs = {
        "script": attr.label(allow_single_file = [".cjs"], mandatory = True),
        "data": attr.label_list(allow_files = True),
        "_node": attr.label(default = "@im_node//:bin/node", executable = True, allow_single_file = True, cfg = "exec"),
        "_typescript": attr.label(default = "@im_typescript//:lib/typescript.js", allow_single_file = True),
        "_compiler": attr.label(default = "@im_typescript//:compiler", allow_files = True),
    },
)
