#!/usr/bin/env python3
"""Deterministically build/check P2-P4 packs and the typed thematic catalog."""
import argparse, hashlib, json, struct, tempfile, shutil
from pathlib import Path
SOURCE_COMMIT="49440f81b6c84700f44082a28494a04dab7b3689"; CORE="philosophy-core-v1"
KIND_TAG={"definition":0,"interpretive_claim":1,"empirical_claim":2,"normative_claim":3,"hypothesis":4}
SPECS={
 "agency-responsibility-v1":{"theme":"agency-responsibility","facts":["fact.freedom_choice","fact.freedom_choice.counterpoint","fact.freedom_choice.consequence","fact.responsibility_accountability","fact.responsibility_accountability.counterpoint"],"relations":[(1,"Counters",0),(4,"Contradicts",3),(0,"Supports",3),(2,"Entails",3),(3,"DependsOn",0),(4,"Qualifies",0),(2,"Supports",0)]},
 "epistemology-truth-v1":{"theme":"epistemology-truth","facts":["fact.truth_reality","fact.truth_reality.counterpoint","fact.truth_reality.consequence","fact.opinion_position","fact.opinion_position.counterpoint"],"relations":[(1,"Counters",0),(4,"Contradicts",3),(2,"Supports",0),(0,"Entails",2),(0,"DependsOn",2),(3,"Qualifies",0),(4,"Counters",0)]},
 "mind-memory-language-v1":{"theme":"mind-memory-language","facts":["fact.memory_information_process","fact.memory_information_process.counterpoint","fact.recollection_new_frame","fact.recollection_new_frame.counterpoint","fact.language_thought"],"relations":[(1,"Counters",0),(3,"Contradicts",2),(2,"Supports",0),(4,"Entails",2),(4,"DependsOn",0),(3,"Qualifies",0),(2,"Supports",4)]},
}
def put(v): b=v.encode() if isinstance(v,str) else v; return struct.pack(">I",len(b))+b
def sha(v): return hashlib.sha256(v).digest()
def thesis_digest(r): return sha(put(b"qxfx0:thesis:canonical:v1")+bytes([1])+put(r["subject"])+put(r["relation"])+put(r["object"])+bytes([KIND_TAG[r["kind"]]])+struct.pack(">I",0)).hex()
def evidence_digest(r): return sha(put(b"qxfx0:evidence-record:v1")+bytes([1])+put(r["id"])+put(r["source_id"])+bytes([1,0])+put(r["source_revision"])+bytes.fromhex(r["content_digest"])+struct.pack(">H",r["strength_basis_points"])).hex()
def set_digest(ds): return sha(put(b"qxfx0:evidence-set:v1")+bytes([1])+struct.pack(">I",len(ds))+b''.join(bytes.fromhex(x) for x in sorted(ds))).hex()
def encoded(v): return (json.dumps(v,ensure_ascii=False,indent=2)+"\n").encode()
def write(p,v): p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(encoded(v))
def build(repo):
 facts={x["record"]["id"]:x["record"] for x in json.loads((repo/"data/packs"/CORE/"facts.json").read_text())}
 for pack_id,spec in sorted(SPECS.items()):
  rows=[]; evidence=[]; links=[]; assessments=[]
  for i,fid in enumerate(spec["facts"]):
   r=facts[fid]; tid=f"{pack_id}:{fid}"; td=thesis_digest(r)
   rows.append({"thesis_id":tid,"authority_fact_id":fid,"thesis_digest":td,"theme":spec["theme"],"tags":sorted([spec["theme"],r["subject"].removeprefix("concept.")])})
   eid=f"{pack_id}:evidence:{fid}"; strength=7000+i*500
   er={"id":eid,"source_id":f"{CORE}:{fid}","kind":"curated_reference","trust_class":"curated_embedded","source_revision":SOURCE_COMMIT,"content_digest":hashlib.sha256(f"{pack_id}:{fid}:curated-reference".encode()).hexdigest(),"strength_basis_points":strength}
   ed=evidence_digest(er); evidence.append({"record":er,"canonical_digest":ed}); links.append({"thesis_id":tid,"evidence_id":eid,"role":"supports"})
   assessments.append({"id":f"{pack_id}:assessment:{fid}","thesis_id":tid,"thesis_digest":td,"policy_version":1,"evidence_set_digest":set_digest([ed]),"confidence_basis_points":10000,"authority_evidence_count":1})
  ds=[r["thesis_digest"] for r in rows]
  relations=[{"from":ds[a],"kind":kind,"to":ds[b]} for a,kind,b in spec["relations"]]
  q=4 if pack_id=="agency-responsibility-v1" else 3
  scenarios=[{"scenario_id":f"{pack_id}:counterargument","action":"Counterargument","head":ds[0],"trigger":ds[1],"result":ds[0],"relation":"Counters","rationale":"An explicit counterargument contests the active authority thesis without replacing its stable identity."},{"scenario_id":f"{pack_id}:revision","action":"Revision","head":ds[0],"trigger":ds[q],"result":ds[2],"relation":"Qualifies","rationale":"A scoped qualification motivates deterministic revision to a distinct existing authority thesis."}]
  files={"theses.json":encoded(rows),"relations.json":encoded(relations),"lifecycle.json":encoded(scenarios),"evidence.json":encoded(evidence),"evidence-links.json":encoded(links),"assessments.json":encoded(assessments)}
  out=repo/"data/packs"/pack_id;out.mkdir(parents=True,exist_ok=True)
  for n,b in files.items():(out/n).write_bytes(b)
  write(out/"manifest.json",{"pack_id":pack_id,"pack_version":1,"schema_version":2,"source_repository":"QxFx0","source_commit":SOURCE_COMMIT,"license":"MIT","dependencies":[CORE],"files":{n:hashlib.sha256(b).hexdigest() for n,b in sorted(files.items())}})
  print(f"built {pack_id}: {len(rows)} theses, {len(evidence)} evidence, {len(assessments)} assessments")
 build_catalog(repo)
def canonical_digest(value, digest_field):
 copy=json.loads(json.dumps(value));copy[digest_field]=""
 if digest_field=="catalog_digest" and "packs" in copy:
  copy["packs"].sort(key=lambda x:x["pack_id"])
  for pack in copy["packs"]:
   for key in ("themes","concept_coverage","thesis_coverage","dependencies"):
    pack[key].sort(key=(lambda x:(x["pack_id"],x["pack_version"],x["manifest_digest"])) if key=="dependencies" else None)
   for key in ("concept_namespaces","thesis_namespaces","overlay_of"): pack["ownership"][key].sort()
 return hashlib.sha256(json.dumps(copy,ensure_ascii=False,separators=(",",":")).encode()).hexdigest()
def build_catalog(repo):
 root=repo/"data/packs"; manifests={}
 for pack_id in [CORE,*sorted(SPECS)]:
  raw=(root/pack_id/"manifest.json").read_bytes(); manifests[pack_id]=hashlib.sha256(raw).hexdigest()
 core_pin={"pack_id":CORE,"pack_version":1,"manifest_digest":manifests[CORE]}
 packs=[{"pack_id":CORE,"pack_version":1,"manifest_digest":manifests[CORE],"themes":["philosophy-core"],"concept_coverage":[x["concept_id"] for x in json.loads((root/CORE/"concepts.json").read_text())],"thesis_coverage":[],"dependencies":[],"trust_tier":"curated_embedded","lifecycle":"approved","ownership":{"concept_namespaces":["concept."],"thesis_namespaces":[],"overlay_of":[]},"license":"MIT","authority_facts":True}]
 for pack_id,spec in sorted(SPECS.items()):
  theses=json.loads((root/pack_id/"theses.json").read_text())
  packs.append({"pack_id":pack_id,"pack_version":1,"manifest_digest":manifests[pack_id],"themes":[spec["theme"]],"concept_coverage":[],"thesis_coverage":[x["thesis_id"] for x in theses],"dependencies":[core_pin],"trust_tier":"curated_embedded","lifecycle":"approved","ownership":{"concept_namespaces":[],"thesis_namespaces":[pack_id+":"],"overlay_of":[CORE]},"license":"MIT","authority_facts":False})
 future=[("affect-action-v1","affect-action","candidate"),("aesthetics-v1","aesthetics","planned"),("ethics-social-order-v1","ethics-social-order","candidate"),("existential-v1","existential","planned"),("temporality-history-v1","temporality-history","planned")]
 for pack_id,theme,lifecycle in future:
  placeholder=hashlib.sha256(("qxfx0:planned-pack:v1:"+pack_id).encode()).hexdigest()
  packs.append({"pack_id":pack_id,"pack_version":1,"manifest_digest":placeholder,"themes":[theme],"concept_coverage":[],"thesis_coverage":[],"dependencies":[],"trust_tier":"discovery_only","lifecycle":lifecycle,"ownership":{"concept_namespaces":[],"thesis_namespaces":[],"overlay_of":[]},"license":"MIT","authority_facts":False})
 catalog={"schema_version":1,"catalog_id":"qxfx0-thematic-catalog-v1","catalog_version":1,"catalog_digest":"","packs":sorted(packs,key=lambda x:x["pack_id"])}
 catalog["catalog_digest"]=canonical_digest(catalog,"catalog_digest")
 out=root/"catalog-v1";write(out/"catalog.json",catalog); raw=(out/"catalog.json").read_bytes()
 manifest={"schema_version":1,"catalog_id":catalog["catalog_id"],"catalog_version":1,"catalog_digest":catalog["catalog_digest"],"files":{"catalog.json":hashlib.sha256(raw).hexdigest()},"manifest_digest":""}
 manifest["manifest_digest"]=canonical_digest(manifest,"manifest_digest");write(out/"manifest.json",manifest)
 print(f"built catalog-v1: {len(packs)} entries, digest={catalog['catalog_digest']}")
def snapshot(root):return {str(p.relative_to(root)):p.read_bytes() for p in sorted(root.rglob('*')) if p.is_file()}
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--repo',type=Path,default=Path(__file__).resolve().parents[1]);ap.add_argument('--check',action='store_true');a=ap.parse_args();repo=a.repo.resolve()
 if not a.check:build(repo);return
 before=snapshot(repo/'data/packs')
 with tempfile.TemporaryDirectory() as d:
  tmp=Path(d);(tmp/'data/packs').mkdir(parents=True);shutil.copytree(repo/'data/packs'/CORE,tmp/'data/packs'/CORE);build(tmp);after=snapshot(tmp/'data/packs')
 expected={k:v for k,v in before.items() if any(k.startswith(x+'/') for x in [CORE,*SPECS,'catalog-v1'])}
 if expected!=after:
  raise SystemExit(f"pack drift: missing={sorted(set(after)-set(expected))}, extra={sorted(set(expected)-set(after))}, changed={sorted(k for k in set(after)&set(expected) if after[k]!=expected[k])}")
 print('thematic pack check: clean')
if __name__=='__main__':main()
