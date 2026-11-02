
class C
{
    void Goo()
    {
        [A]
        goto bar;
        bar:
            Goo();
    }
}