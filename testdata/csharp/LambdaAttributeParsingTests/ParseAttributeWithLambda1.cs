// Lambda inside attribute with attributes of its own will cause us to bail out.
[A([B]() => {})]
class C
{
}