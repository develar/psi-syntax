
// Note: this is weird, but is supported by language.  It checks just that the ValueText is `dynamic`, not the raw text.
using D = @dynamic;

class C
{
    void M(D d)
    {
        d.Goo();
    }
}