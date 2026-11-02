
class C
{
    void Goo()
    {
        [A]await using (null) { }
    }
}