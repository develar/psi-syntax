
class C
{
    void M()
    {
        void local<T>() where T : /*comment*/ enum /*comment*/ { }
    }
}
