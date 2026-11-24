# This comment-only line is longer than the configured line length, so it is reported only when comments are not ignored.
def f():
    # This indented comment-only line is also longer than the configured line length, so the same applies to it.
    x = 1  # This trailing comment pushes the line over the limit, but the code before it is short.
    return x


# Code that is too long on its own is always reported.
very_long_variable_name = some_function_name(first_argument, second_argument, third_arg_x)
very_long_variable_name = some_function_name(first_argument, second_argument, third_arg_x)  # comment

# Text inside a string is not a comment.
message = "# This is not a comment, even though it starts with a hash, and it is too long."
