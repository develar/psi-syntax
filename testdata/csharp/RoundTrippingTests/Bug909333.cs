
extern alias libAlias;
class myClass
{
static int Main()
{
libAlias::otherClass oc = new libAlias.otherClass(); // '::' and '.'
return 0;
}
}
