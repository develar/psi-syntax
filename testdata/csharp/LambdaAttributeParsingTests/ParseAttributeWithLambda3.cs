class C
{
    void M()
    {
        // Because we're already in an expression, parsing an attribute list, we'll bail out of parsing the `[B]`
        // as an attribute on an inner lambda.
        var v = [A([B]() => {})]
            () => {};
    }
}