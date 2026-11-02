
class C
{
    C() : base() => M();
    C() => M();
    ~C() => M();
    int P { set => M(); }
}
