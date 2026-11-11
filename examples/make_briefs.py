"""Editorial fixtures; sources are supplied, not fetched/verified by the engine."""
import json
from pathlib import Path

def slide(kind,headline,message,source,**extra):return dict(kind=kind,headline=headline,message=message,source=source,**extra)
def deck(name,title,direction,audience,purpose,emotion,slides):
 p=Path('examples/briefs')/f'{name}.json';p.write_text(json.dumps(dict(title=title,direction=direction,audience=audience,purpose=purpose,emotion=emotion,slides=slides),indent=2))
science='OpenStax Biology 2e, sections 5.2–5.3: https://openstax.org/books/biology-2e/pages/5-2-passive-transport'
deck('science','Across the cell membrane','scientific','Secondary-school biology learners','Explain passive and active transport','Curiosity through causal explanation',[
 slide('opener','A boundary.\nNot a barrier.','The cell membrane regulates exchange between a cell and its surroundings.',science),
 slide('mechanism','Diffusion follows a concentration gradient.','Net movement follows the gradient; individual particles continue to move in both directions.',science,steps=['Particles move randomly.','More particles leave the crowded side.','Net movement declines toward equilibrium.']),
 slide('comparison','Not every molecule crosses the same way.','Permeability depends on the molecule and the membrane.',science,columns=[dict(title='Simple diffusion',body='Small nonpolar molecules can pass through the lipid bilayer.'),dict(title='Facilitated diffusion',body='Transport proteins help selected solutes move down their gradients.')]),
 slide('statement','Against the gradient?\nEnergy is required.','Active transport couples movement to an energy source. It is not simply faster diffusion.',science),
 slide('closing','Ask what drives\nthe movement.','Identify the solute, the membrane pathway and the direction of the concentration gradient.',science)])
business='Author-authored decision framework; recommendations, not empirical business findings.'
deck('strategy','Strategy as a set of choices','swiss','Leadership team','Frame a strategy workshop without invented performance claims','Decisive, analytical',[
 slide('opener','Choose where\nnot to compete.','A workshop framework for turning broad ambition into explicit choices.',business),
 slide('comparison','Separate aspiration from a decision.','An aspiration describes intent; a decision commits resources and accepts a trade-off.',business,columns=[dict(title='Aspiration',body='“Be the most useful partner.” Useful as a direction, insufficient as a resource choice.'),dict(title='Decision',body='Prioritize one customer problem, name the owner and define what will be deferred.')]),
 slide('mechanism','Make the assumptions visible.','Treat strategic choices as hypotheses to investigate, not forecasts disguised as facts.',business,steps=['Name a customer problem.','State the proposed advantage.','Identify disconfirming evidence.']),
 slide('statement','A roadmap is not\nthe strategy.','A delivery sequence is useful only when it serves a clearly stated choice.',business),
 slide('closing','Leave with a choice,\nnot another slogan.','Record the commitment, the trade-off, the accountable owner and the date for review.',business)])
tech='Original concept scenario, not a real product, customer result, benchmark or release announcement.'
deck('product','Fieldnote / product concept','industrial','Design and engineering reviewers','Present a fictional offline research tool concept','Focused, grounded in use',[
 slide('opener','Capture the field.\nKeep the context.','Fieldnote is a concept for organizing research observations when connectivity is unreliable.',tech),
 slide('mechanism','Design around the research session.','The proposed workflow keeps the observation and its context together.',tech,steps=['Capture notes locally.','Attach time and context.','Review before syncing.']),
 slide('comparison','Offline-first is a product decision.','The architecture must make state and uncertainty visible to the researcher.',tech,columns=[dict(title='During collection',body='Show local save state. Avoid suggesting that an unsynced note is backed up remotely.'),dict(title='During reconciliation',body='Expose conflicts. Let the researcher resolve meaning rather than silently replacing text.')]),
 slide('statement','Trust is a visible\nsystem state.','A save indicator should communicate what has actually happened, not merely that a button was pressed.',tech),
 slide('closing','Validate the workflow\nbefore the feature list.','Next research: observe a field session, prototype interruptions and test recovery with participants.',tech)])
arch='Original conceptual pavilion study; design intent only, not a built project or engineering certification.'
deck('architecture','A pavilion for pause','architectural','Architecture studio reviewers','Communicate a conceptual spatial sequence','Calm, tactile, reflective',[
 slide('opener','Make room\nfor stillness.','A conceptual pavilion study organized around arrival, shelter and a framed view.',arch),
 slide('mechanism','The space unfolds in three moments.','Sequence directs attention before any material or ornament is added.',arch,steps=['A compressed threshold.','An open sheltered room.','A framed view outward.']),
 slide('comparison','Structure and enclosure play different roles.','This distinction is conceptual; structural design requires project-specific engineering.',arch,columns=[dict(title='Primary structure',body='Define a legible load path and a restrained set of supporting elements.'),dict(title='Secondary enclosure',body='Filter light and frame views without obscuring the spatial organization.')]),
 slide('statement','Light is a material\nthat changes.','Study the opening across different sun angles before fixing its proportions.',arch),
 slide('closing','Test the space,\nnot just the image.','Next studies: circulation, accessibility, climate response and a physical or digital section model.',arch)])
history='General overview: Encyclopaedia Britannica, history of printing: https://www.britannica.com/technology/printing/History-of-printing'
deck('history','The printed page','museum','General history learners','Explain printing as a system rather than a lone-inventor story','Reflective, historically contextual',[
 slide('opener','A page can travel\nfarther than a voice.','Printing connects technologies, materials, labour and systems of circulation.',history),
 slide('timeline','Printing has more than one origin.','These selected milestones are not a complete global history.',history,events=[dict(date='7th–9th c.',text='Woodblock printing develops in East Asia.'),dict(date='11th c.',text='Bi Sheng develops movable type in China.'),dict(date='15th c.',text='Metal movable type and press printing expand in Europe.')]),
 slide('mechanism','A technology becomes a system.','Repeatability depends on more than the shape of a printing press.',history,steps=['Prepare type and page.','Transfer ink to paper.','Distribute and read copies.']),
 slide('comparison','More copies do not mean equal access.','Language, cost, literacy and institutions shape who can use printed material.',history,columns=[dict(title='Production',body='Making copies can increase the availability of a text.'),dict(title='Access',body='Availability does not remove the social and economic barriers to reading.')]),
 slide('closing','Ask who made it.\nAsk who could read it.','Study a printed object as evidence of both a production process and a social world.',history)])
