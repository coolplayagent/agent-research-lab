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
