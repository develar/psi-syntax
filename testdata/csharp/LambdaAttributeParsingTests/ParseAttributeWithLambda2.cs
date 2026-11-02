// Lambda inside attribute without attributes of its own is fine for parsing.
[A(() => {})]
class C
{
}