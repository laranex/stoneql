import {syntaxTree} from '@codemirror/language';
import type {EditorState} from '@codemirror/state';
import type {SyntaxNode} from '@lezer/common';
import type {SqlSchemaIndex,IdentifierPart,SqlObjectMetadata,SqlColumnMetadata} from './schema.ts';
import {identifierMatches} from './dialects.ts';
export interface SqlToken extends IdentifierPart {from:number;to:number;kind:string;depth:number;group:number;text:string;identifier:boolean}
export interface SqlBinding {alias:IdentifierPart;object?:SqlObjectMetadata}
export interface SqlContext {tokens:SqlToken[];before:SqlToken[];bindings:SqlBinding[];selectAliases:string[];hasFrom:boolean;clause:string;from:number;to:number;prefix:string;quote?:string;qualifier:IdentifierPart[];activeString?:SqlToken;blocked:boolean}
interface Group {id:number;parent:number;from:number;to:number;depth:number;query:boolean}
const cache=new WeakMap<object,{tree:object;statementFrom:number;statementTo:number;tokens:SqlToken[];groups:Group[]}>();
function decodeIdentifier(text:string){const quote=text[0];if(['`','"','['].includes(quote||'')){const close=quote==='['?']':quote!;return text.slice(1,text.endsWith(close)?-1:undefined).replaceAll(close+close,close);}return text;}
function statementAt(node:SyntaxNode,pos:number):SyntaxNode|undefined {for(let current:SyntaxNode|null=node;current;current=current.parent)if(current.name==='Statement')return current;return;}
function analysis(state:EditorState,pos:number){const tree=syntaxTree(state),statement=statementAt(tree.resolveInner(pos,-1),pos);const from=statement?.from??pos,to=statement?.to??pos;const saved=cache.get(state.doc);if(saved&&saved.tree===tree&&saved.statementFrom===from&&saved.statementTo===to)return saved;
 const groups:Group[]=[{id:0,parent:0,from,to,depth:0,query:true}],stack=[0],tokens:SqlToken[]=[];let budget=4000;
 tree.iterate({from,to,enter(node){if(!budget--||node.from>from+65536)return false;if(node.node.firstChild)return;if(/Comment|⚠/.test(node.name)||node.from===node.to)return;
 const text=state.sliceDoc(node.from,node.to);if(node.name===')'&&stack.length>1){groups[stack.at(-1)!]!.to=node.to;stack.pop();}
 const group=groups[stack.at(-1)!]!,identifier=node.name==='Identifier'||node.name==='QuotedIdentifier';tokens.push({from:node.from,to:node.to,kind:node.name,text,value:identifier?decodeIdentifier(text):text,quoted:node.name==='QuotedIdentifier',identifier,depth:group.depth,group:group.id});
 if(node.name==='('){const id=groups.length;groups.push({id,parent:group.id,from:node.from,to,depth:group.depth+1,query:false});stack.push(id);}
 }});
 for(const group of groups.slice(1)){const first=tokens.find(token=>token.group===group.id);group.query=!!first&&/^(SELECT|WITH)$/i.test(first.text);}
 const result={tree,statementFrom:from,statementTo:to,tokens,groups};cache.set(state.doc,result);return result;
}
function readPath(tokens:SqlToken[],start:number){const parts:IdentifierPart[]=[];let i=start;if(!tokens[i]?.identifier)return {parts,end:i};parts.push(tokens[i]!);i++;while(tokens[i]?.text==='.'&&tokens[i+1]?.identifier){parts.push(tokens[i+1]!);i+=2;}return {parts,end:i};}
export function referenceBefore(tokens:SqlToken[],end=tokens.length):IdentifierPart[]{let i=end-1;if(!tokens[i]?.identifier)return [];const result:IdentifierPart[]=[tokens[i]!];while(i>=2&&tokens[i-1]?.text==='.'&&tokens[i-2]?.identifier){result.unshift(tokens[i-2]!);i-=2;}return result;}
export function detectContext(state:EditorState,pos:number,index:SqlSchemaIndex):SqlContext{
 const data=analysis(state,pos),engine=index.metadata.engine;const leaf=syntaxTree(state).resolveInner(pos,-1);let queryGroup=data.groups.filter(g=>g.query&&g.from<=pos&&g.to>=pos).sort((a,b)=>b.depth-a.depth)[0]??data.groups[0]!;
 const scopeOf=(id:number):number=>{while(!data.groups[id]!.query)id=data.groups[id]!.parent;return id;};
 let tokens=data.tokens.filter(t=>scopeOf(t.group)===queryGroup.id);
 // UNION arms have independent table aliases; never leak one arm into another.
 const unions=tokens.filter(t=>t.depth===queryGroup.depth&&/^(UNION|INTERSECT|EXCEPT)$/i.test(t.text));const left=unions.filter(t=>t.to<=pos).at(-1)?.to??-1,right=unions.find(t=>t.from>pos)?.from??Infinity;tokens=tokens.filter(t=>t.from>=left&&t.to<=right);
 const bindings:SqlBinding[]=[],selectAliases:string[]=[];let hasFrom=false,inFrom=false,sourceDepth=queryGroup.depth;
 for(let i=0;i<tokens.length;i++){const token=tokens[i]!;if(token.depth!==sourceDepth)continue;const upper=token.text.toUpperCase();
 if(['FROM','JOIN','UPDATE','INTO'].includes(upper)||inFrom&&token.text===','){
   if(upper==='FROM')hasFrom=true;if(upper==='FROM'||upper==='JOIN')inFrom=true;const path=readPath(tokens,i+1);if(!path.parts.length)continue;const object=index.resolve(path.parts);let alias=path.parts.at(-1)!;let next=path.end;if(tokens[next]?.text.toUpperCase()==='AS')next++;if(tokens[next]?.identifier)alias=tokens[next]!;bindings.push({alias:{value:engine==='PostgreSQL'&&!alias.quoted?alias.value.toLowerCase():alias.value,quoted:alias.quoted},object});i=path.end-1;
 } else if(['WHERE','GROUP','ORDER','HAVING','LIMIT','OFFSET','RETURNING','SET','VALUES'].includes(upper))inFrom=false;
 if(upper==='AS'&&!hasFrom&&tokens[i+1]?.identifier)selectAliases.push(tokens[i+1]!.value);
 }
 const current=tokens.find(t=>t.from<pos&&t.to>=pos&&t.kind===leaf.name&&t.from===leaf.from);
 let from=pos,to=pos,prefix='',quote:string|undefined,activeString:SqlToken|undefined,blocked=false;
 if(/Comment/.test(leaf.name)&&pos<=leaf.to){const raw=state.sliceDoc(leaf.from,leaf.to);blocked=!(leaf.name==='BlockComment'&&pos===leaf.to&&raw.endsWith('*/'));}
 if(current?.kind==='String'){const start=current.text[0],closed=current.text.length>1&&current.text.endsWith(start)&&pos===current.to;if(!closed){activeString=current;from=current.from;to=current.to;prefix=state.sliceDoc(from,pos);}}
 else if(current&&(current.identifier||['Keyword','Builtin','Type'].includes(current.kind))){from=current.from;to=current.to;prefix=state.sliceDoc(from,pos);if(current.quoted){quote=current.text[0];prefix=decodeIdentifier(prefix);}}
 const before=tokens.filter(t=>t.to<=from);const qualifier:IdentifierPart[]=[];
 if(before.at(-1)?.text==='.'){let cursor=before.length-2;while(before[cursor]?.identifier){qualifier.unshift(before[cursor]!);if(before[cursor-1]?.text!=='.')break;cursor-=2;}}
 let clause='';for(let i=0;i<before.length;i++){const token=before[i]!;if(token.depth!==queryGroup.depth)continue;const upper=token.text.toUpperCase();if(['SELECT','FROM','JOIN','ON','WHERE','HAVING','SET','VALUES','UPDATE','INTO','LIMIT','RETURNING'].includes(upper))clause=upper;else if((upper==='GROUP'||upper==='ORDER')&&before[i+1]?.text.toUpperCase()==='BY')clause=upper+' BY';}
 return {tokens,before,bindings,selectAliases,hasFrom,clause,from,to,prefix,quote,qualifier,activeString,blocked};
}
export function resolveColumn(parts:IdentifierPart[],context:SqlContext,index:SqlSchemaIndex,selectedTable?:string):{column:SqlColumnMetadata;object:SqlObjectMetadata}|undefined{
 const column=parts.at(-1);if(!column)return;let objects:SqlObjectMetadata[]=[];
 if(parts.length>1){const qualifier=parts.slice(0,-1),alias=qualifier.length===1?context.bindings.filter(binding=>identifierMatches(binding.alias.value,qualifier[0]!.value,qualifier[0]!.quoted,index.metadata.engine)):[];if(alias.length===1){if(alias[0]!.object)objects=[alias[0]!.object!];}else if(!alias.length){const object=index.resolve(qualifier);if(object)objects=[object];}}
 else {if(context.bindings.some(binding=>!binding.object))return;objects=context.bindings.flatMap(binding=>binding.object?[binding.object]:[]);if(!objects.length&&!context.hasFrom&&selectedTable){const object=index.resolve([{value:selectedTable,quoted:true}]);if(object)objects=[object];}}
 const matches=objects.flatMap(object=>object.columns.filter(c=>identifierMatches(c.name,column.value,column.quoted,index.metadata.engine)).map(column=>({column,object})));return matches.length===1?matches[0]:undefined;
}
export function expectedEnum(context:SqlContext,index:SqlSchemaIndex,selectedTable?:string){if(!['WHERE','HAVING','ON','SET','VALUES'].includes(context.clause))return;let lhs:IdentifierPart[]=[];const before=context.before,last=before.at(-1)?.text.toUpperCase();
 if(['=','!=','<>'].includes(last||''))lhs=referenceBefore(before,before.length-1);
 else {let open=-1,depth=0;for(let i=before.length-1;i>=0;i--){if(before[i]!.text===')')depth++;if(before[i]!.text==='('){if(depth)depth--;else{open=i;break;}}}if(open>=1&&before[open-1]!.text.toUpperCase()==='IN'&&before.slice(open+1).every(t=>t.kind==='String'||t.text===','))lhs=referenceBefore(before,open-1);}
 const found=resolveColumn(lhs,context,index,selectedTable);return found?.column.enumValues?found:undefined;
}
